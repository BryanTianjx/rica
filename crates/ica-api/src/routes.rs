//! Route table — the 18 endpoints R0 + W1 serve.
//!
//! Every handler is a thin adapter: validate the query/body, call one use case,
//! map the result. The interesting rules live in `ica-application`, the SQL in
//! `ica-infra`, and the wire shapes in `dto.rs`.
//!
//! Request bodies are taken as `Bytes` and parsed by `body::parse` so a
//! malformed body produces the project's JSON 422 envelope instead of axum's
//! plain-text extractor error. Queries go through [`ApiQuery`](crate::query::ApiQuery)
//! for the same reason, and the parity middlewares in `http_parity` keep the
//! 405/HEAD/trailing-slash answers on Starlette's side of the line.

use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{middleware, Json, Router};

use crate::body;
use crate::dto::{
    CandidateDetailRead, CandidateRead, ConversationRead, HealthResponse, MessageRead,
    PageResponse, PositionRead, SendRecordRead, TimelineEntryRead,
};
use crate::error::ApiError;
use crate::http_parity::RoutePatterns;
use crate::query::{
    self, ApiQuery, RawListCandidatesQuery, RawListConversationsQuery, RawListMessagesQuery,
    RawListPositionsQuery, RawListSendRecordsQuery, RawTimelineQuery,
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
    Ok(Json(PageResponse::from_page(page, |position| {
        PositionRead::from(position)
    })))
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
    Ok(Json(PageResponse::from_page(page, |candidate| {
        CandidateRead::from(candidate)
    })))
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
    Ok(Json(PageResponse::from_page(page, |conversation| {
        ConversationRead::from(conversation)
    })))
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
    Ok(Json(PageResponse::from_page(page, |message| {
        MessageRead::from(message)
    })))
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
    Ok(Json(PageResponse::from_page(page, |record| {
        SendRecordRead::from(record)
    })))
}

/// Unknown path: keep the project's error envelope instead of axum's plain
/// text 404, so a client never has to parse two error formats.
async fn unknown_path() -> ApiError {
    ApiError::unknown_path()
}
