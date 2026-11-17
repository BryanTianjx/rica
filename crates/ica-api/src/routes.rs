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
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{middleware, Json, Router};

use crate::body;
use crate::dto::{
    CandidateDetailRead, CandidateRead, ConversationRead, DashboardRead, FollowUpDecisionRead,
    FollowUpPolicyRead, FollowUpTaskRead, HealthResponse, MessageRead, PageResponse, PositionRead,
    SendRecordRead, TimelineEntryRead,
};
use crate::error::ApiError;
use crate::extract::ApiQuery;
use crate::http_parity::RoutePatterns;
use crate::query::{
    self, RawFollowUpCancelQuery, RawListCandidatesQuery, RawListConversationsQuery,
    RawListFollowUpsQuery, RawListMessagesQuery, RawListPositionsQuery, RawListSendRecordsQuery,
    RawTimelineQuery,
};
use crate::state::AppState;

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
