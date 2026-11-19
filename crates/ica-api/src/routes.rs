//! Route table — the 29 endpoints R0 + W1 + W2 serve.
//!
//! Every handler is a thin adapter: validate the query/body, call one use case,
//! map the result. The interesting rules live in `ica-application`, the SQL in
//! `ica-infra`, and the wire shapes in `dto.rs`.
//!
//! Request bodies are taken as `Bytes` and parsed by `body::parse` so a
//! malformed body produces the project's JSON 422 envelope instead of axum's
//! plain-text extractor error. Queries go through [`ApiQuery`](crate::extract::ApiQuery)
//! for the same reason, and the parity middlewares in `http_parity` keep the
//! 405/HEAD/trailing-slash answers on Starlette's side of the line.

use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{get, post};
use axum::{middleware, Json, Router};

use crate::body;
use crate::dto::{
    CandidateDetailRead, CandidateRead, ContextPreviewRead, ConversationRead, DashboardRead,
    DraftCopyResponse, DraftGroupRead, DraftRead, EncryptedExportResponse, ExportResponse,
    FollowUpDecisionRead, FollowUpPolicyRead, FollowUpTaskRead, HealthResponse,
    ImportConfirmResult, ImportOcrResult, ImportResult, ImportSessionRead, MessageRead,
    PageResponse, PositionRead, SendRecordRead, TimelineEntryRead,
};
use crate::error::ApiError;
use crate::extract::ApiQuery;
use crate::http_parity::RoutePatterns;
use crate::query::{
    self, RawFollowUpCancelQuery, RawListCandidatesQuery, RawListConversationsQuery,
    RawListDraftsQuery, RawListFollowUpsQuery, RawListMessagesQuery, RawListPositionsQuery,
    RawListSendRecordsQuery, RawTimelineQuery,
};
use crate::state::AppState;
use ica_contracts::error::ContractError;

/// Every path the router serves, as axum 0.8 patterns.
///
/// The parity middlewares (`http_parity::reject_head`,
/// `http_parity::redirect_trailing_slash`) test against this list instead of
/// inspecting the router, which axum does not expose. Keep it in sync with
/// `router` below: every entry must be registered, and every registration must
/// appear here.
const ROUTE_PATTERNS: &[&str] = &[
    "/health",
    "/api/positions",
    "/api/positions/{position_id}",
    "/api/candidates",
    "/api/candidates/{candidate_id}",
    "/api/candidates/{candidate_id}/timeline",
    "/api/candidates/{candidate_id}/context",
    "/api/conversations",
    "/api/messages",
    "/api/send-records",
    "/api/follow-ups/evaluate",
    "/api/follow-ups/policy/current",
    "/api/follow-ups",
    "/api/follow-ups/{task_id}",
    "/api/follow-ups/{task_id}/cancel",
    "/api/follow-ups/{task_id}/complete",
    "/api/follow-ups/{task_id}/record-result",
    "/api/dashboard",
    "/api/drafts",
    "/api/drafts/generate",
    "/api/drafts/{draft_id}",
    "/api/drafts/{draft_id}/approve",
    "/api/drafts/{draft_id}/reject",
    "/api/drafts/{draft_id}/copy",
    "/api/draft-groups/{group_id}",
    "/api/draft-groups/{group_id}/select",
    // W4
    "/api/imports/preview",
    "/api/imports/ocr",
    "/api/imports/{session_id}",
    "/api/imports/{session_id}/confirm",
    "/api/imports/{session_id}/confirm-candidate",
    "/api/imports/{session_id}/reject",
    "/api/export",
    "/api/import",
];

/// Build the router for `state`.
pub fn router(state: AppState) -> Router {
    let patterns: RoutePatterns = Arc::from(ROUTE_PATTERNS);
    let head_patterns = patterns.clone();
    Router::new()
        .route("/health", get(health))
        .route("/api/positions", get(list_positions).post(create_position))
        .route(
            "/api/positions/{position_id}",
            get(get_position)
                .patch(update_position)
                .delete(delete_position),
        )
        .route(
            "/api/candidates",
            get(list_candidates).post(create_candidate),
        )
        .route(
            "/api/candidates/{candidate_id}",
            get(get_candidate)
                .patch(update_candidate)
                .delete(delete_candidate),
        )
        .route(
            "/api/candidates/{candidate_id}/timeline",
            get(candidate_timeline),
        )
        .route(
            "/api/conversations",
            get(list_conversations).post(create_conversation),
        )
        .route("/api/messages", get(list_messages).post(create_message))
        .route(
            "/api/send-records",
            get(list_send_records).post(record_send),
        )
        .route("/api/follow-ups/evaluate", post(follow_up_evaluate))
        .route(
            "/api/follow-ups/policy/current",
            get(get_follow_up_policy).patch(update_follow_up_policy),
        )
        .route(
            "/api/follow-ups",
            get(list_follow_ups).post(create_follow_up),
        )
        .route(
            "/api/follow-ups/{task_id}",
            get(get_follow_up).patch(update_follow_up),
        )
        .route("/api/follow-ups/{task_id}/cancel", post(cancel_follow_up))
        .route(
            "/api/follow-ups/{task_id}/complete",
            post(complete_follow_up),
        )
        .route(
            "/api/follow-ups/{task_id}/record-result",
            post(record_follow_up_result),
        )
        .route("/api/dashboard", get(dashboard))
        .route("/api/drafts", get(list_drafts))
        // Static segments must be registered before the `{draft_id}` param
        // route for readability; axum's matcher prioritises them either way.
        .route("/api/drafts/generate", post(generate_draft))
        .route("/api/drafts/{draft_id}", get(get_draft).patch(update_draft))
        .route("/api/drafts/{draft_id}/approve", post(approve_draft))
        .route("/api/drafts/{draft_id}/reject", post(reject_draft))
        .route("/api/drafts/{draft_id}/copy", post(copy_draft))
        .route(
            "/api/candidates/{candidate_id}/context",
            get(preview_context),
        )
        .route("/api/draft-groups/{group_id}", get(get_draft_group))
        .route("/api/imports/preview", post(preview_import))
        // The literal `ocr` segment is registered next to `preview`; the
        // `{session_id}` routes below must not swallow it.
        .route("/api/imports/ocr", post(ocr_import))
        .route("/api/imports/{session_id}", get(get_import))
        .route("/api/imports/{session_id}/confirm", post(confirm_import))
        .route(
            "/api/imports/{session_id}/confirm-candidate",
            post(confirm_candidate_import),
        )
        .route("/api/imports/{session_id}/reject", post(reject_import))
        .route("/api/export", post(export_data))
        .route("/api/import", post(import_data))
        .route(
            "/api/draft-groups/{group_id}/select",
            post(select_draft_from_group),
        )
        .fallback(unknown_path)
        .method_not_allowed_fallback(|| async { crate::http_parity::method_not_allowed() })
        // `reject_head` must sit *inside* the redirect (added first = run last):
        // Starlette redirects `/api/positions/` before it ever considers the
        // method, so a `HEAD` with a trailing slash is a 307, not a 405.
        .layer(middleware::from_fn(move |request, next| {
            crate::http_parity::reject_head(head_patterns.clone(), request, next)
        }))
        .layer(middleware::from_fn(move |request, next| {
            crate::http_parity::redirect_trailing_slash(patterns.clone(), request, next)
        }))
        .with_state(state)
}

/// `GET /health` — the liveness probe the desktop shell polls.
async fn health(State(state): State<AppState>) -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        app: state.config().app_name.clone(),
        ai_enabled: state.config().ai_enabled,
    })
}

// ── Positions ──────────────────────────────────────────────────────────────

/// `GET /api/positions` — one page of positions, newest `created_at` first.
///
/// `total` counts every row that matched the filter (before the window), so the
/// UI can render "1-50 of 213" and decide whether to show a next-page control.
async fn list_positions(
    State(state): State<AppState>,
    ApiQuery(raw): ApiQuery<RawListPositionsQuery>,
) -> Result<Json<PageResponse<PositionRead>>, ApiError> {
    let query = query::validate(raw)?;
    let page = ica_application::list_positions(state.positions(), query).await?;
    Ok(Json(PageResponse::from_page(page, PositionRead::from)))
}

/// `POST /api/positions` — create one, 201 with the stored row.
async fn create_position(
    State(state): State<AppState>,
    body: Bytes,
) -> Result<(StatusCode, Json<PositionRead>), ApiError> {
    let raw: body::RawPositionCreate = body::parse(&body)?;
    let new = body::position_create(raw)?;
    let position = ica_application::positions::create_position(state.positions(), new).await?;
    Ok((StatusCode::CREATED, Json(PositionRead::from(position))))
}

/// `GET /api/positions/{position_id}` — one position, archived or not.
async fn get_position(
    State(state): State<AppState>,
    Path(position_id): Path<String>,
) -> Result<Json<PositionRead>, ApiError> {
    let position = ica_application::get_position(state.positions(), &position_id).await?;
    Ok(Json(PositionRead::from(position)))
}

/// `PATCH /api/positions/{position_id}` — apply the present fields only.
///
/// An explicit `null` on `name`/`archived` is a 422 before this point (IMP-2);
/// `status: null` is ignored, exactly like the baseline.
async fn update_position(
    State(state): State<AppState>,
    Path(position_id): Path<String>,
    body: Bytes,
) -> Result<Json<PositionRead>, ApiError> {
    let raw: body::RawPositionPatch = body::parse(&body)?;
    let patch = body::position_patch(raw)?;
    let position =
        ica_application::positions::update_position(state.positions(), &position_id, patch).await?;
    Ok(Json(PositionRead::from(position)))
}

/// `DELETE /api/positions/{position_id}` — 204, candidates keep their row.
async fn delete_position(
    State(state): State<AppState>,
    Path(position_id): Path<String>,
) -> Result<StatusCode, ApiError> {
    ica_application::positions::delete_position(state.positions(), &position_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

// ── Candidates ─────────────────────────────────────────────────────────────

/// `GET /api/candidates` — one page, `updated_at DESC`, with the real total.
async fn list_candidates(
    State(state): State<AppState>,
    ApiQuery(raw): ApiQuery<RawListCandidatesQuery>,
) -> Result<Json<PageResponse<CandidateRead>>, ApiError> {
    let query = query::validate_candidates(raw)?;
    let page = ica_application::candidates::list_candidates(state.candidates(), query).await?;
    Ok(Json(PageResponse::from_page(page, CandidateRead::from)))
}

/// `POST /api/candidates` — create one; a missing position is a 404.
async fn create_candidate(
    State(state): State<AppState>,
    body: Bytes,
) -> Result<(StatusCode, Json<CandidateRead>), ApiError> {
    let raw: body::RawCandidateCreate = body::parse(&body)?;
    let new = body::candidate_create(raw)?;
    let candidate =
        ica_application::candidates::create_candidate(state.positions(), state.candidates(), new)
            .await?;
    Ok((StatusCode::CREATED, Json(CandidateRead::from(candidate))))
}

/// `GET /api/candidates/{candidate_id}` — the candidate plus its position.
async fn get_candidate(
    State(state): State<AppState>,
    Path(candidate_id): Path<String>,
) -> Result<Json<CandidateDetailRead>, ApiError> {
    let detail =
        ica_application::candidates::get_candidate_detail(state.candidates(), &candidate_id)
            .await?;
    Ok(Json(CandidateDetailRead {
        candidate: CandidateRead::from(detail.candidate),
        position: detail.position.map(PositionRead::from),
    }))
}

/// `PATCH /api/candidates/{candidate_id}` — `position_id: null` detaches.
async fn update_candidate(
    State(state): State<AppState>,
    Path(candidate_id): Path<String>,
    body: Bytes,
) -> Result<Json<CandidateRead>, ApiError> {
    let raw: body::RawCandidatePatch = body::parse(&body)?;
    let patch = body::candidate_patch(raw)?;
    let candidate = ica_application::candidates::update_candidate(
        state.positions(),
        state.candidates(),
        &candidate_id,
        patch,
    )
    .await?;
    Ok(Json(CandidateRead::from(candidate)))
}

/// `DELETE /api/candidates/{candidate_id}` — 204, FK cascade removes children.
async fn delete_candidate(
    State(state): State<AppState>,
    Path(candidate_id): Path<String>,
) -> Result<StatusCode, ApiError> {
    ica_application::candidates::delete_candidate(state.candidates(), &candidate_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

// ── Conversations, messages and timeline ───────────────────────────────────

/// `GET /api/conversations` — `last_message_at DESC NULLS LAST`.
async fn list_conversations(
    State(state): State<AppState>,
    ApiQuery(raw): ApiQuery<RawListConversationsQuery>,
) -> Result<Json<PageResponse<ConversationRead>>, ApiError> {
    let query = query::validate_conversations(raw)?;
    let page =
        ica_application::conversations::list_conversations(state.conversations(), query).await?;
    Ok(Json(PageResponse::from_page(page, ConversationRead::from)))
}

/// `POST /api/conversations` — open a thread and audit `conversation.created`.
async fn create_conversation(
    State(state): State<AppState>,
    body: Bytes,
) -> Result<(StatusCode, Json<ConversationRead>), ApiError> {
    let raw: body::RawConversationCreate = body::parse(&body)?;
    let new = body::conversation_create(raw)?;
    let conversation = ica_application::conversations::create_conversation(
        state.candidates(),
        state.conversations(),
        new,
    )
    .await?;
    Ok((
        StatusCode::CREATED,
        Json(ConversationRead::from(conversation)),
    ))
}

/// `GET /api/messages` — time order, real `total` (IMP-1).
async fn list_messages(
    State(state): State<AppState>,
    ApiQuery(raw): ApiQuery<RawListMessagesQuery>,
) -> Result<Json<PageResponse<MessageRead>>, ApiError> {
    let query = query::validate_messages(raw)?;
    let page = ica_application::conversations::list_messages(state.messages(), query).await?;
    Ok(Json(PageResponse::from_page(page, MessageRead::from)))
}

/// `POST /api/messages` — record one, opening or reusing the conversation.
async fn create_message(
    State(state): State<AppState>,
    body: Bytes,
) -> Result<(StatusCode, Json<MessageRead>), ApiError> {
    let raw: body::RawMessageCreate = body::parse(&body)?;
    let new = body::message_create(raw)?;
    let message = ica_application::conversations::create_message(
        state.candidates(),
        state.conversations(),
        state.messages(),
        new,
    )
    .await?;
    Ok((StatusCode::CREATED, Json(MessageRead::from(message))))
}

/// `GET /api/candidates/{candidate_id}/timeline` — an array, not a `Page`.
async fn candidate_timeline(
    State(state): State<AppState>,
    Path(candidate_id): Path<String>,
    ApiQuery(raw): ApiQuery<RawTimelineQuery>,
) -> Result<Json<Vec<TimelineEntryRead>>, ApiError> {
    let limit = query::validate_timeline(raw)?;
    let entries = ica_application::conversations::candidate_timeline(
        state.candidates(),
        state.messages(),
        &candidate_id,
        limit,
    )
    .await?;
    Ok(Json(
        entries.into_iter().map(TimelineEntryRead::from).collect(),
    ))
}

// ── Send records ───────────────────────────────────────────────────────────

/// `POST /api/send-records` — append one manual send, audited.
async fn record_send(
    State(state): State<AppState>,
    body: Bytes,
) -> Result<(StatusCode, Json<SendRecordRead>), ApiError> {
    let raw: body::RawSendRecordCreate = body::parse(&body)?;
    let new = body::send_record_create(raw)?;
    let record =
        ica_application::send_records::record_send(state.candidates(), state.send_records(), new)
            .await?;
    Ok((StatusCode::CREATED, Json(SendRecordRead::from(record))))
}

/// `GET /api/send-records` — newest send first, four filters.
async fn list_send_records(
    State(state): State<AppState>,
    ApiQuery(raw): ApiQuery<RawListSendRecordsQuery>,
) -> Result<Json<PageResponse<SendRecordRead>>, ApiError> {
    let query = query::validate_send_records(raw)?;
    let page =
        ica_application::send_records::list_send_records(state.send_records(), query).await?;
    Ok(Json(PageResponse::from_page(page, SendRecordRead::from)))
}

/// Unknown path: keep the project's error envelope instead of axum's plain
/// text 404, so a client never has to parse two error formats.
async fn unknown_path() -> ApiError {
    ApiError::unknown_path()
}

// ── Follow-ups and dashboard (W2) ──────────────────────────────────────────

/// `POST /api/follow-ups/evaluate` — the decision engine's answer, 200.
///
/// Not a 201: the endpoint creates nothing (the baseline declares 200).
async fn follow_up_evaluate(
    State(state): State<AppState>,
    body: Bytes,
) -> Result<Json<FollowUpDecisionRead>, ApiError> {
    let raw: body::RawFollowUpEvaluate = body::parse(&body)?;
    let evaluation = body::follow_up_evaluate(raw)?;
    let decision = ica_application::evaluate(
        state.follow_ups(),
        state.candidates(),
        state.positions(),
        evaluation,
    )
    .await?;
    Ok(Json(FollowUpDecisionRead::from(decision)))
}

/// `GET /api/follow-ups` — one page of tasks; stale rows are swept first.
async fn list_follow_ups(
    State(state): State<AppState>,
    ApiQuery(raw): ApiQuery<RawListFollowUpsQuery>,
) -> Result<Json<PageResponse<FollowUpTaskRead>>, ApiError> {
    let query = query::validate_follow_ups(raw)?;
    let filter = ica_application::FollowUpTaskFilter {
        candidate_id: query.candidate_id,
        status: query.status,
    };
    let (tasks, total) =
        ica_application::list_tasks(state.follow_ups(), filter, query.offset, query.limit).await?;
    let page = ica_domain::Page {
        items: tasks,
        total,
        offset: query.offset,
        limit: query.limit,
    };
    Ok(Json(PageResponse::from_page(page, FollowUpTaskRead::from)))
}

/// `POST /api/follow-ups` — create (or reuse) a task, 201 with the row.
async fn create_follow_up(
    State(state): State<AppState>,
    body: Bytes,
) -> Result<(StatusCode, Json<FollowUpTaskRead>), ApiError> {
    let raw: body::RawFollowUpCreate = body::parse(&body)?;
    let new = body::follow_up_create(raw)?;
    let task = ica_application::create_task(
        state.follow_ups(),
        state.candidates(),
        state.positions(),
        new,
    )
    .await?;
    Ok((StatusCode::CREATED, Json(FollowUpTaskRead::from(task))))
}

/// `GET /api/follow-ups/{task_id}` — one task.
async fn get_follow_up(
    State(state): State<AppState>,
    Path(task_id): Path<String>,
) -> Result<Json<FollowUpTaskRead>, ApiError> {
    let task = ica_application::get_task(state.follow_ups(), &task_id).await?;
    Ok(Json(FollowUpTaskRead::from(task)))
}

/// `PATCH /api/follow-ups/{task_id}` — patch fields; the state machine
/// validates any status change before the row is written.
async fn update_follow_up(
    State(state): State<AppState>,
    Path(task_id): Path<String>,
    body: Bytes,
) -> Result<Json<FollowUpTaskRead>, ApiError> {
    let raw: body::RawFollowUpPatch = body::parse(&body)?;
    let patch = body::follow_up_patch(raw)?;
    let task = ica_application::update_task(state.follow_ups(), &task_id, patch).await?;
    Ok(Json(FollowUpTaskRead::from(task)))
}

/// `POST /api/follow-ups/{task_id}/cancel` — a 200 self-transition on an
/// already-cancelled task, 409 from a closed terminal state.
async fn cancel_follow_up(
    State(state): State<AppState>,
    Path(task_id): Path<String>,
    ApiQuery(raw): ApiQuery<RawFollowUpCancelQuery>,
) -> Result<Json<FollowUpTaskRead>, ApiError> {
    let task = ica_application::cancel_task(state.follow_ups(), &task_id, raw.reason).await?;
    Ok(Json(FollowUpTaskRead::from(task)))
}

/// `POST /api/follow-ups/{task_id}/complete` — 200 from `Copied`,
/// `AwaitingResult` and `Completed` (self), 409 elsewhere.
async fn complete_follow_up(
    State(state): State<AppState>,
    Path(task_id): Path<String>,
) -> Result<Json<FollowUpTaskRead>, ApiError> {
    let task = ica_application::complete_task(state.follow_ups(), &task_id).await?;
    Ok(Json(FollowUpTaskRead::from(task)))
}

/// `POST /api/follow-ups/{task_id}/record-result` — the only operation that
/// can mark a message as actually sent; appends the send log row.
async fn record_follow_up_result(
    State(state): State<AppState>,
    Path(task_id): Path<String>,
    body: Bytes,
) -> Result<Json<FollowUpTaskRead>, ApiError> {
    let raw: body::RawFollowUpResult = body::parse(&body)?;
    let result = body::follow_up_result(raw)?;
    let task = ica_application::record_result(state.follow_ups(), &task_id, result).await?;
    Ok(Json(FollowUpTaskRead::from(task)))
}

/// `GET /api/follow-ups/policy/current` — creates the default row on first use.
async fn get_follow_up_policy(
    State(state): State<AppState>,
) -> Result<Json<FollowUpPolicyRead>, ApiError> {
    let record = ica_application::ensure_default_policy(state.follow_ups()).await?;
    Ok(Json(FollowUpPolicyRead::from(record)))
}

/// `PATCH /api/follow-ups/policy/current` — thresholds with pydantic ranges.
async fn update_follow_up_policy(
    State(state): State<AppState>,
    body: Bytes,
) -> Result<Json<FollowUpPolicyRead>, ApiError> {
    let raw: body::RawFollowUpPolicyPatch = body::parse(&body)?;
    let patch = body::follow_up_policy_patch(raw)?;
    let record = ica_application::update_policy(state.follow_ups(), patch).await?;
    Ok(Json(FollowUpPolicyRead::from(record)))
}

/// `GET /api/dashboard` — the aggregated workbench, after the maintenance tick.
async fn dashboard(State(state): State<AppState>) -> Result<Json<DashboardRead>, ApiError> {
    use ica_domain::CandidateStatus;
    let snapshot = ica_application::dashboard(state.dashboard()).await?;
    let mut status_counts: std::collections::BTreeMap<String, i64> =
        std::collections::BTreeMap::new();
    // The baseline seeds every enumeration key with 0 before filling counts.
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
    Ok(Json(DashboardRead {
        candidate_total: snapshot.candidate_total,
        candidate_status_counts: status_counts,
        active_follow_up_count: snapshot.active_follow_up_count,
        pending_follow_ups: snapshot
            .pending_follow_ups
            .into_iter()
            .map(FollowUpTaskRead::from)
            .collect(),
        awaiting_result_count: snapshot.awaiting_result_count,
        pending_draft_count: snapshot.pending_draft_count,
        recent_messages: snapshot
            .recent_messages
            .into_iter()
            .map(MessageRead::from)
            .collect(),
        position_total: snapshot.position_total,
        open_position_count: snapshot.open_position_count,
    }))
}

// ── Drafts and draft groups (W3) ───────────────────────────────────────────

/// `GET /api/drafts` — one page of drafts, `created_at DESC`.
///
/// `total` is the true filtered count (IMP-3's uniform rule), so the approval
/// centre can render "1-50 of N" without loading the whole table.
async fn list_drafts(
    State(state): State<AppState>,
    ApiQuery(raw): ApiQuery<RawListDraftsQuery>,
) -> Result<Json<PageResponse<DraftRead>>, ApiError> {
    let query = query::validate_drafts(raw)?;
    let (drafts, total) =
        ica_application::list_drafts(state.drafts(), query.filter, query.offset, query.limit)
            .await?;
    let page = ica_domain::Page {
        items: drafts,
        total,
        offset: query.offset,
        limit: query.limit,
    };
    Ok(Json(PageResponse::from_page(page, DraftRead::from)))
}

/// `GET /api/drafts/{draft_id}` — one draft.
async fn get_draft(
    State(state): State<AppState>,
    Path(draft_id): Path<String>,
) -> Result<Json<DraftRead>, ApiError> {
    let draft = ica_application::get_draft(state.drafts(), &draft_id).await?;
    Ok(Json(DraftRead::from(draft)))
}

/// `POST /api/drafts/generate` — 201 with the created draft.
///
/// With no provider configured the AI port answers the baseline's
/// `503 ai_provider_error`, which propagates before anything is persisted.
async fn generate_draft(
    State(state): State<AppState>,
    body: Bytes,
) -> Result<(StatusCode, Json<DraftRead>), ApiError> {
    let raw: body::RawGenerateRequest = body::parse(&body)?;
    let request = body::generate_request(raw)?;
    let outcome = ica_application::generate(
        state.drafts(),
        state.follow_ups(),
        state.candidates(),
        state.positions(),
        state.draft_generator(),
        request,
    )
    .await?;
    match outcome {
        ica_application::drafts::GenerateOutcome::Created(draft) => {
            Ok((StatusCode::CREATED, Json(DraftRead::from(*draft))))
        }
    }
}

/// `GET /api/candidates/{candidate_id}/context` — what the model would see.
async fn preview_context(
    State(state): State<AppState>,
    Path(candidate_id): Path<String>,
) -> Result<Json<ContextPreviewRead>, ApiError> {
    let context = ica_application::preview_context(
        state.candidates(),
        state.positions(),
        state.messages(),
        &candidate_id,
    )
    .await?;
    Ok(Json(ContextPreviewRead::from(context)))
}

/// `PATCH /api/drafts/{draft_id}` — edit the wording.
async fn update_draft(
    State(state): State<AppState>,
    Path(draft_id): Path<String>,
    body: Bytes,
) -> Result<Json<DraftRead>, ApiError> {
    let raw: body::RawDraftPatch = body::parse(&body)?;
    let patch = body::draft_patch(raw)?;
    let draft = ica_application::update_draft(state.drafts(), &draft_id, patch).await?;
    Ok(Json(DraftRead::from(draft)))
}

/// `POST /api/drafts/{draft_id}/approve` — approval only, never a send.
async fn approve_draft(
    State(state): State<AppState>,
    Path(draft_id): Path<String>,
    body: Bytes,
) -> Result<Json<DraftRead>, ApiError> {
    let raw: body::RawDraftApprove = body::parse(&body)?;
    let note = body::draft_approve(raw)?;
    let draft =
        ica_application::approve_draft(state.drafts(), state.follow_ups(), &draft_id, note).await?;
    Ok(Json(DraftRead::from(draft)))
}

/// `POST /api/drafts/{draft_id}/reject` — no guard, every state answers 200.
async fn reject_draft(
    State(state): State<AppState>,
    Path(draft_id): Path<String>,
) -> Result<Json<DraftRead>, ApiError> {
    let draft =
        ica_application::reject_draft(state.drafts(), state.follow_ups(), &draft_id).await?;
    Ok(Json(DraftRead::from(draft)))
}

/// `POST /api/drafts/{draft_id}/copy` — copy is not a send.
async fn copy_draft(
    State(state): State<AppState>,
    Path(draft_id): Path<String>,
) -> Result<Json<DraftCopyResponse>, ApiError> {
    let outcome =
        ica_application::copy_draft(state.drafts(), state.follow_ups(), &draft_id).await?;
    Ok(Json(DraftCopyResponse::from(outcome)))
}

/// `GET /api/draft-groups/{group_id}` — the group, `created_at ASC`.
async fn get_draft_group(
    State(state): State<AppState>,
    Path(group_id): Path<String>,
) -> Result<Json<DraftGroupRead>, ApiError> {
    let drafts = ica_application::get_draft_group(state.drafts(), &group_id).await?;
    Ok(Json(build_draft_group(drafts)))
}

/// `POST /api/draft-groups/{group_id}/select` — pick one, reject the rest.
async fn select_draft_from_group(
    State(state): State<AppState>,
    Path(group_id): Path<String>,
    body: Bytes,
) -> Result<Json<DraftGroupRead>, ApiError> {
    let raw: body::RawDraftGroupSelect = body::parse(&body)?;
    let selection = body::draft_group_select(raw)?;
    let drafts = ica_application::select_draft_from_group(
        state.drafts(),
        state.follow_ups(),
        &group_id,
        selection,
    )
    .await?;
    Ok(Json(build_draft_group(drafts)))
}

/// `_build_draft_group`: the group id, the **newest** `created_at`, and the
/// drafts in `created_at ASC` order (the use case already sorted them).
fn build_draft_group(drafts: Vec<ica_domain::GeneratedDraft>) -> DraftGroupRead {
    let draft_group_id = drafts
        .first()
        .and_then(|draft| draft.draft_group_id.clone())
        .unwrap_or_default();
    let created_at = drafts.iter().map(|draft| draft.created_at).max();
    DraftGroupRead {
        draft_group_id,
        created_at,
        drafts: drafts.into_iter().map(DraftRead::from).collect(),
    }
}

// ── W4: imports, export, import, OCR ────────────────────────────────────────

/// `POST /api/imports/preview` — parse pasted text into an import session.
///
/// 200, not 201: the baseline declares no `status_code` on this route, and the
/// preview is a reading of the page rather than a new addressable resource.
async fn preview_import(
    State(state): State<AppState>,
    body: Bytes,
) -> Result<Json<ImportSessionRead>, ApiError> {
    let raw: body::RawImportPreview = body::parse(&body)?;
    let request = body::import_preview(raw)?;
    let session = ica_application::preview_import(state.imports(), request).await?;
    Ok(Json(ImportSessionRead::from(session)))
}

/// `GET /api/imports/{session_id}` — one session with its parsed records.
async fn get_import(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> Result<Json<ImportSessionRead>, ApiError> {
    let session = state.imports().find(&session_id).await?.ok_or_else(|| {
        ApiError::from(ContractError::not_found(format!(
            "Import session '{session_id}' was not found"
        )))
    })?;
    Ok(Json(ImportSessionRead::from(session)))
}

/// `POST /api/imports/{session_id}/confirm` — write the parsed records into the
/// candidate's timeline.
async fn confirm_import(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    body: Bytes,
) -> Result<Json<ImportConfirmResult>, ApiError> {
    let raw: body::RawImportConfirm = body::parse(&body)?;
    let request = body::import_confirm(raw)?;
    let outcome = ica_application::confirm_import(state.imports(), &session_id, request).await?;
    Ok(Json(ImportConfirmResult {
        session_id: outcome.session_id,
        saved_message_ids: outcome.saved_message_ids,
        skipped_count: outcome.skipped_count,
        duplicate_count: outcome.duplicate_count,
    }))
}

/// `POST /api/imports/{session_id}/confirm-candidate` — write a REVIEWED
/// browser extraction draft into the candidate archive. This is the ONLY path
/// from a page into `candidates` (baseline §6.2): the browser router can draft
/// and review, never write. The service refuses a draft that no human
/// approved, whose approval digest no longer matches, or that duplicates an
/// existing candidate without an explicit override (AC-04-4).
async fn confirm_candidate_import(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    body: Bytes,
) -> Result<Json<serde_json::Value>, ApiError> {
    let raw: body::RawCandidateConfirm = body::parse(&body)?;
    let request = body::candidate_confirm(raw)?;
    let outcome = ica_application::confirm_candidate(
        state.imports(),
        state.extractions(),
        &session_id,
        ica_application::CandidateConfirm {
            position_id: request.position_id,
            status: request.status,
            confirm_duplicate: request.confirm_duplicate,
            note: request.note,
        },
    )
    .await?;
    Ok(Json(serde_json::json!({
        "session_id": outcome.session_id,
        "extraction_id": outcome.extraction_id,
        "candidate_id": outcome.candidate_id,
        "duplicate_kind": outcome.duplicate_kind,
        "duplicate_override": outcome.duplicate_override,
        "fields_written": outcome.fields_written,
    })))
}

/// `POST /api/imports/{session_id}/reject` — close a session without importing.
async fn reject_import(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> Result<Json<ImportSessionRead>, ApiError> {
    let session = ica_application::reject_import(state.imports(), &session_id).await?;
    Ok(Json(ImportSessionRead::from(session)))
}

/// `POST /api/export` — the whole snapshot, optionally sealed.
///
/// Two response shapes, chosen by whether `X-Encrypt-Aes-Key` is present:
/// without it the documents go out in the body; with it they are replaced by
/// the AES-256-GCM envelope. The audit row is written by the use case **after**
/// sealing succeeds.
async fn export_data(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    let key = headers
        .get("x-encrypt-aes-key")
        .and_then(|value| value.to_str().ok())
        .map(|value| value.to_string());

    let outcome =
        ica_application::export_data(state.snapshots(), state.export_sealer(), key.as_deref())
            .await?;

    let body = match outcome {
        ica_application::sync::ExportOutcome::Plain(snapshot) => {
            serde_json::to_value(ExportResponse {
                version: snapshot.version,
                exported_at: snapshot.exported_at,
                data: snapshot.data,
            })
            .map_err(|error| {
                ApiError::from(ContractError::internal(format!(
                    "export could not be serialised: {error}"
                )))
            })?
        }
        ica_application::sync::ExportOutcome::Sealed(envelope) => {
            serde_json::to_value(EncryptedExportResponse {
                // `SealedSnapshot` carries no algorithm field: the sealer is
                // AES-256-GCM by contract, so the marker is fixed.
                alg: "A256GCM".to_string(),
                iv: envelope.iv,
                tag: envelope.tag,
                ciphertext: envelope.ciphertext,
            })
            .map_err(|error| {
                ApiError::from(ContractError::internal(format!(
                    "sealed export could not be serialised: {error}"
                )))
            })?
        }
    };
    Ok(Json(body))
}

/// `POST /api/import` — merge a snapshot, last-write-wins per row.
async fn import_data(
    State(state): State<AppState>,
    body: Bytes,
) -> Result<Json<ImportResult>, ApiError> {
    let raw: body::RawImportPayload = body::parse(&body)?;
    let data = body::import_payload(raw)?;
    let outcome = ica_application::import_data(state.snapshots(), data).await?;
    Ok(Json(ImportResult {
        imported: outcome.imported,
        updated: outcome.updated,
        skipped: outcome.skipped,
        conflicts: outcome.conflicts,
    }))
}

/// `POST /api/imports/ocr` — read a screenshot from the staging directory.
///
/// The path guard runs before any I/O, so a path outside the staging area is
/// refused without disclosing whether the file exists.
async fn ocr_import(
    State(state): State<AppState>,
    body: Bytes,
) -> Result<Json<ImportOcrResult>, ApiError> {
    let raw: body::RawImportOcr = body::parse(&body)?;
    let request = body::import_ocr(raw)?;
    let candidate_id = request.candidate_id.clone();
    let outcome = ica_application::ocr_screenshot(
        state.imports(),
        state.ocr(),
        request,
        &state.config().ocr_upload_dir,
    )
    .await?;
    Ok(Json(ImportOcrResult {
        session_id: outcome.session.id,
        text: outcome.text,
        language: outcome.language,
        candidate_id,
        created_at: outcome.session.created_at,
    }))
}
