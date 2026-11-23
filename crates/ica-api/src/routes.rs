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
use axum::routing::{delete, get, post, put};
use axum::{middleware, Json, Router};

use crate::body;
use crate::dto::{
    AIEgressRead, AiBudgetRead, AiKeyStatus, AiModelsRead, AiRoutingRead, AiUsageRead,
    AuditEventRead, BackupRead, CandidateDetailRead, CandidateRead, ContextPreviewRead,
    ConversationRead, DashboardCalendar, DashboardRead, DraftCopyResponse, DraftGroupRead,
    DraftRead, EncryptedExportResponse, ExportResponse, FollowUpDecisionRead, FollowUpPolicyRead,
    FollowUpTaskRead, HealthResponse, ImportConfirmResult, ImportOcrResult, ImportResult,
    ImportSessionRead, MessageRead, PageResponse, PositionRead, RestoreRead, SendRecordRead,
    SettingsRead, ShareRead, TimelineEntryRead, TokenResponse, UserRead,
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
    // W5 (first slice)
    "/api/audit-events",
    // W5 settings slice
    "/api/settings",
    "/api/settings/ai-egress",
    "/api/settings/ai-egress/consent",
    // W5 AI slice
    "/api/ai/models",
    "/api/ai/routing",
    "/api/ai/usage",
    "/api/ai/budget",
    // W5 ai-key + backups
    "/api/settings/ai-key",
    "/api/backups",
    "/api/backups/{name}/restore",
    // W6 auth + shares
    "/api/auth/register",
    "/api/auth/login",
    "/api/auth/refresh",
    "/api/auth/logout",
    "/api/auth/me",
    "/api/shares",
    "/api/shares/{share_id}",
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
        // W5 (first slice): the audit trail, newest first.
        .route("/api/audit-events", get(audit_events))
        // W5 settings slice.
        .route("/api/settings", get(read_settings))
        .route("/api/settings/ai-egress", get(read_ai_egress))
        .route(
            "/api/settings/ai-egress/consent",
            post(write_ai_egress_consent),
        )
        // W5 AI slice (model CRUD is AI-EXT, deliberately absent).
        .route("/api/ai/models", get(list_ai_models))
        .route(
            "/api/ai/routing",
            get(get_ai_routing).patch(update_ai_routing),
        )
        .route("/api/ai/usage", get(get_ai_usage))
        .route("/api/ai/budget", get(get_ai_budget).patch(update_ai_budget))
        // W5 ai-key + backups.
        .route(
            "/api/settings/ai-key",
            put(write_ai_key_endpoint).delete(clear_ai_key_endpoint),
        )
        .route(
            "/api/backups",
            get(list_backups_endpoint).post(create_backup_endpoint),
        )
        .route("/api/backups/{name}/restore", post(restore_backup_endpoint))
        // W6 auth + shares (local mode is the default: no middleware gates
        // the other routes, `me` answers the synthetic local user).
        .route("/api/auth/register", post(register_endpoint))
        .route("/api/auth/login", post(login_endpoint))
        .route("/api/auth/refresh", post(refresh_endpoint))
        .route("/api/auth/logout", post(logout_endpoint))
        .route("/api/auth/me", get(me_endpoint))
        .route(
            "/api/shares",
            post(create_share_endpoint).get(list_shares_endpoint),
        )
        .route("/api/shares/{share_id}", delete(delete_share_endpoint))
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
        calendar: DashboardCalendar {
            month: snapshot.calendar_month,
            messages_by_day: snapshot.calendar_messages_by_day.into_iter().collect(),
            followups_by_day: snapshot.calendar_followups_by_day.into_iter().collect(),
        },
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

/// The generate response: one draft, or the whole A/B comparison group —
/// the baseline returns `draft_to_read(...)` or the plain list, so the
/// untagged enum keeps both wire shapes.
#[derive(serde::Serialize)]
#[serde(untagged)]
enum GenerateResponse {
    Single(Box<DraftRead>),
    Group(Vec<DraftRead>),
}

/// `POST /api/drafts/generate` — 201 with the created draft (or the whole
/// group when `comparison_models` is set).
///
/// With no provider configured the AI port answers the baseline's
/// `503 ai_provider_error`, which propagates before anything is persisted.
async fn generate_draft(
    State(state): State<AppState>,
    body: Bytes,
) -> Result<(StatusCode, Json<GenerateResponse>), ApiError> {
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
        ica_application::drafts::GenerateOutcome::Created(draft) => Ok((
            StatusCode::CREATED,
            Json(GenerateResponse::Single(Box::new(DraftRead::from(*draft)))),
        )),
        ica_application::drafts::GenerateOutcome::Group(drafts) => Ok((
            StatusCode::CREATED,
            Json(GenerateResponse::Group(
                drafts.into_iter().map(DraftRead::from).collect(),
            )),
        )),
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
    // The 404 and the repository error both come from the shared use case.
    let session = ica_application::get_import(state.imports(), &session_id).await?;
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

// ── W5: audit events ───────────────────────────────────────────────────────

/// `GET /api/audit-events` — the audit trail, newest first (a bare array,
/// like the timeline; the baseline's `read_audit_events`).
///
/// The trail is what makes the send boundary verifiable: it shows which
/// action put a task into which state, so "the draft was copied but never
/// sent" is a fact you can check rather than a claim. Only small metadata is
/// stored — never chat bodies, keys or tokens.
async fn audit_events(
    State(state): State<AppState>,
    ApiQuery(raw): ApiQuery<query::RawListAuditEventsQuery>,
) -> Result<Json<Vec<AuditEventRead>>, ApiError> {
    let list = query::validate_audit_events(raw)?;
    let events = ica_application::recent_audit_events(state.audit_trail(), list.into()).await?;
    Ok(Json(events.into_iter().map(AuditEventRead::from).collect()))
}

// ── W5: settings ───────────────────────────────────────────────────────────

/// `GET /settings` — the effective configuration, minus any secret
/// (the baseline's `read_settings`).
async fn read_settings(State(state): State<AppState>) -> Json<SettingsRead> {
    let snapshot = state.settings_config_view().settings_snapshot();
    Json(SettingsRead {
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

/// `GET /settings/ai-egress` — where AI context would go and what is masked
/// before it leaves (`PRIV-07`). Read-only, never carries a key.
async fn read_ai_egress(State(state): State<AppState>) -> Result<Json<AIEgressRead>, ApiError> {
    let disclosure =
        ica_application::read_egress_disclosure(state.settings(), &state.settings_config_view())
            .await?;
    Ok(Json(disclosure.into()))
}

/// `POST /settings/ai-egress/consent` — turn outbound AI context on or off,
/// audit the change, and return the fresh disclosure. `consent_granted` is
/// required and explicit: an omitted body must never silently turn egress on.
async fn write_ai_egress_consent(
    State(state): State<AppState>,
    body: Bytes,
) -> Result<Json<AIEgressRead>, ApiError> {
    let raw: body::RawAiEgressConsent = body::parse(&body)?;
    let consent = body::ai_egress_consent(raw)?;
    let disclosure = ica_application::write_egress_consent(
        state.settings(),
        &state.settings_config_view(),
        consent.consent_granted,
    )
    .await?;
    Ok(Json(disclosure.into()))
}

// == W5: AI models / routing / usage / budget ==

/// `GET /ai/models` — the registry with the V1.0 fallback, key presence only.
async fn list_ai_models(State(state): State<AppState>) -> Result<Json<AiModelsRead>, ApiError> {
    let overview = ica_application::list_ai_models(
        state.ai_models(),
        state.secrets(),
        &state.ai_settings_view(),
    )
    .await?;
    Ok(Json(AiModelsRead {
        models: overview.models.into_iter().map(Into::into).collect(),
        routing: overview.routing,
    }))
}

/// `GET /ai/routing` — the effective table (defaults layered with overrides).
async fn get_ai_routing(State(state): State<AppState>) -> Result<Json<AiRoutingRead>, ApiError> {
    let routing = ica_application::get_ai_routing(state.ai_models()).await?;
    Ok(Json(AiRoutingRead { routing }))
}

/// `PATCH /ai/routing` — persist overrides for known message types only.
async fn update_ai_routing(
    State(state): State<AppState>,
    body: Bytes,
) -> Result<Json<AiRoutingRead>, ApiError> {
    let raw: body::RawAiRoutingPatch = body::parse(&body)?;
    let patch = body::ai_routing_patch(raw)?;
    let routing = ica_application::update_ai_routing(state.ai_models(), patch.routing).await?;
    Ok(Json(AiRoutingRead { routing }))
}

/// `GET /ai/usage` — today, `days` of history, all-time per model.
async fn get_ai_usage(
    State(state): State<AppState>,
    ApiQuery(raw): ApiQuery<query::RawAiUsageQuery>,
) -> Result<Json<AiUsageRead>, ApiError> {
    let days = query::validate_ai_usage_days(raw)?;
    let today = chrono::Utc::now()
        .naive_utc()
        .format("%Y-%m-%d")
        .to_string();
    let overview =
        ica_application::get_ai_usage(state.ai_usage(), &state.ai_budget(), days, &today).await?;
    Ok(Json(overview.into()))
}

/// `GET /ai/budget` — the current budget config and spend state.
async fn get_ai_budget(State(state): State<AppState>) -> Result<Json<AiBudgetRead>, ApiError> {
    let today = chrono::Utc::now()
        .naive_utc()
        .format("%Y-%m-%d")
        .to_string();
    let spend = ica_application::today_spend_cents(state.ai_usage(), &today).await?;
    Ok(Json(
        ica_application::budget_read(&state.ai_budget(), spend).into(),
    ))
}

/// `PATCH /ai/budget` — persist the pair (rows the baseline never reads
/// back, without an audit event), mutate the in-process values, and answer
/// with the fresh spend state.
async fn update_ai_budget(
    State(state): State<AppState>,
    body: Bytes,
) -> Result<Json<AiBudgetRead>, ApiError> {
    let raw: body::RawAiBudgetPatch = body::parse(&body)?;
    let patch = body::ai_budget_patch(raw)?;
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
        .await?;
    state.set_ai_budget(ica_application::AiBudgetSettings {
        daily_budget_cents: patch.daily_budget_cents,
        warning_threshold: patch.warning_threshold,
    });
    let today = chrono::Utc::now()
        .naive_utc()
        .format("%Y-%m-%d")
        .to_string();
    let spend = ica_application::today_spend_cents(state.ai_usage(), &today).await?;
    Ok(Json(
        ica_application::budget_read(&state.ai_budget(), spend).into(),
    ))
}

// ── W5: ai-key + backups ───────────────────────────────────────────────────

/// `PUT /settings/ai-key` — persist the key through the OS credential store,
/// audit it masked, and report the enabled flag (never the key).
async fn write_ai_key_endpoint(
    State(state): State<AppState>,
    body: Bytes,
) -> Result<Json<AiKeyStatus>, ApiError> {
    let raw: body::RawAiKeyWrite = body::parse(&body)?;
    let write = body::ai_key_write(raw)?;
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
    Ok(Json(AiKeyStatus {
        ai_enabled: state.ai_key_present(),
        key_masked: masked,
        model_id,
    }))
}

/// `DELETE /settings/ai-key` — clear the chosen slot (`?model_id=` targets
/// one model's slot; without it the installation-wide key is cleared) and
/// audit the change.
async fn clear_ai_key_endpoint(
    State(state): State<AppState>,
    ApiQuery(raw): ApiQuery<query::RawAiKeyClearQuery>,
) -> Result<Json<AiKeyStatus>, ApiError> {
    let model_id = query::validate_ai_key_clear(raw)?;
    let (installation, masked, model_id) =
        ica_application::clear_ai_key(state.secrets(), state.settings(), model_id.as_deref())
            .await?;
    if installation && model_id.is_none() {
        state.set_ai_key_present(false);
    }
    Ok(Json(AiKeyStatus {
        ai_enabled: state.ai_key_present(),
        key_masked: masked,
        model_id,
    }))
}

/// `GET /api/backups` — both backup families, newest first.
async fn list_backups_endpoint(
    State(state): State<AppState>,
) -> Result<Json<Vec<BackupRead>>, ApiError> {
    let records = ica_application::list_backups(state.backups()).await?;
    Ok(Json(records.into_iter().map(Into::into).collect()))
}

/// `POST /api/backups` — one consistent online-backup copy of the live
/// database.
async fn create_backup_endpoint(
    State(state): State<AppState>,
) -> Result<Json<BackupRead>, ApiError> {
    let record = ica_application::create_backup(state.backups()).await?;
    Ok(Json(record.into()))
}

/// `POST /api/backups/{name}/restore` — replace the live data with the
/// backup's contents. `confirm` must be explicit; the name is a file name,
/// never a path.
async fn restore_backup_endpoint(
    State(state): State<AppState>,
    Path(name): Path<String>,
    body: Bytes,
) -> Result<Json<RestoreRead>, ApiError> {
    // Business rules (confirm, file-name guard, existence pre-check) live in
    // the shared Application use case; IPC executes the identical code.
    let payload: serde_json::Value = body::parse(&body)?;
    let confirmed = payload["confirm"].as_bool().unwrap_or(false);
    let outcome =
        ica_application::restore_backup_checked(state.backups(), &name, confirmed).await?;
    Ok(Json(RestoreRead {
        restored_from: outcome.restored_from,
        database: outcome.database,
        previous_copy: outcome.previous_copy,
        message: "The database was restored. The previous state was kept as a safety copy next to the backup.".to_string(),
    }))
}

// ── W6: auth + shares ──────────────────────────────────────────────────────

/// `POST /auth/register` — first sign-up bootstraps the tenant as admin.
async fn register_endpoint(
    State(state): State<AppState>,
    body: Bytes,
) -> Result<(StatusCode, Json<UserRead>), ApiError> {
    let raw: body::RawAuthRegister = body::parse(&body)?;
    let write = body::auth_register(raw)?;
    let user = ica_application::register(
        state.auth_repo(),
        state.auth_security(),
        &write.email,
        &write.password,
        write.display_name.as_deref(),
        write.tenant_id.as_deref(),
    )
    .await?;
    Ok((StatusCode::CREATED, Json(UserRead::from(&user))))
}

/// `POST /auth/login` — verify credentials, issue the pair.
async fn login_endpoint(
    State(state): State<AppState>,
    body: Bytes,
) -> Result<Json<TokenResponse>, ApiError> {
    let raw: body::RawAuthLogin = body::parse(&body)?;
    let login = body::auth_login(raw)?;
    let issued = ica_application::login(
        state.auth_repo(),
        state.auth_security(),
        &login.email,
        &login.password,
    )
    .await?;
    Ok(Json(TokenResponse {
        access_token: issued.access_token,
        token_type: "bearer".to_string(),
        expires_in: issued.expires_in,
        refresh_token: issued.refresh_token,
        user: UserRead::from(&issued.user),
    }))
}

/// `POST /auth/refresh` — single-use rotation into a fresh pair.
async fn refresh_endpoint(
    State(state): State<AppState>,
    body: Bytes,
) -> Result<Json<TokenResponse>, ApiError> {
    let token = body::auth_refresh(body::parse(&body)?)?;
    let issued = ica_application::refresh(state.auth_repo(), state.auth_security(), &token).await?;
    Ok(Json(TokenResponse {
        access_token: issued.access_token,
        token_type: "bearer".to_string(),
        expires_in: issued.expires_in,
        refresh_token: issued.refresh_token,
        user: UserRead::from(&issued.user),
    }))
}

/// `POST /auth/logout` — revoke one refresh token, idempotently (204).
async fn logout_endpoint(
    State(state): State<AppState>,
    body: Bytes,
) -> Result<StatusCode, ApiError> {
    // The logout body mirrors refresh: `refresh_token` required, min 1.
    let token = body::auth_refresh(body::parse(&body)?)?;
    ica_application::logout(state.auth_repo(), state.auth_security(), &token).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `GET /auth/me` — the current user; local mode answers the synthetic one.
async fn me_endpoint() -> Json<UserRead> {
    Json(UserRead::from(&ica_application::local_user()))
}

/// `POST /shares` — grant (or upgrade) one recipient's access. The local
/// user is the owner.
async fn create_share_endpoint(
    State(state): State<AppState>,
    body: Bytes,
) -> Result<(StatusCode, Json<ShareRead>), ApiError> {
    let raw: body::RawShareCreate = body::parse(&body)?;
    let create = body::share_create(raw)?;
    // The trusted identity travels in a RequestContext; authorization runs in
    // the Application use case on BOTH transports.
    let ctx = ica_application::RequestContext::local();
    let share = ica_application::create_share(
        state.auth_repo(),
        &ctx,
        &create.recipient_id,
        &create.resource_type,
        &create.resource_id,
        &create.permission,
    )
    .await?;
    Ok((StatusCode::CREATED, Json(ShareRead::from(&share))))
}

/// `GET /shares` — the user's shares (granted or received), newest first.
async fn list_shares_endpoint(
    State(state): State<AppState>,
) -> Result<Json<Vec<ShareRead>>, ApiError> {
    let ctx = ica_application::RequestContext::local();
    let shares = ica_application::list_shares(state.auth_repo(), &ctx).await?;
    Ok(Json(shares.iter().map(ShareRead::from).collect()))
}

/// `DELETE /shares/{id}` — owner or admin, decided by the Application layer.
async fn delete_share_endpoint(
    State(state): State<AppState>,
    Path(share_id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let ctx = ica_application::RequestContext::local();
    ica_application::delete_share(state.auth_repo(), &ctx, &share_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// SEC-04-A AC3 — route-manifest assertions only. No routing behaviour is
/// changed here: the tests pin that the matrix's paths and public whitelist
/// describe the routes actually registered by [`router`].
#[cfg(test)]
mod sec04_a_route_manifest_tests {
    use super::ROUTE_PATTERNS;

    const MATRIX: &str = include_str!("../../../docs/architecture/api-authentication-matrix.md");

    /// The three public endpoints (two contract rows + the liveness probe)
    /// must be real registrations, not documentation fiction.
    #[test]
    fn public_whitelist_is_registered() {
        for path in ["/health", "/api/auth/register", "/api/auth/login"] {
            assert!(
                ROUTE_PATTERNS.contains(&path),
                "SEC-04-A public endpoint {path} is missing from the route manifest"
            );
        }
    }

    /// Every one of the 68 contract rows resolves to a registered axum path;
    /// the extra `confirm-candidate` route is the documented D-2 helper.
    #[test]
    fn every_matrix_contract_path_is_a_registered_route() {
        const EXTRA: &str = "/api/imports/{session_id}/confirm-candidate";
        let mut documented: Vec<String> = Vec::new();
        let mut rows = 0usize;
        for line in MATRIX.lines() {
            let trimmed = line.trim();
            if !trimmed.starts_with('|') {
                continue;
            }
            let cells: Vec<&str> = trimmed
                .trim_matches('|')
                .split('|')
                .map(str::trim)
                .collect();
            if cells.len() < 3 || cells[0].parse::<u32>().is_err() {
                continue;
            }
            rows += 1;
            let path = cells[2].trim_matches('`');
            let registered = format!("/api{path}");
            assert!(
                ROUTE_PATTERNS.contains(&registered.as_str()),
                "matrix contract {} has no matching route registration: {path}",
                cells[0]
            );
            documented.push(registered);
        }
        assert_eq!(rows, 68, "the matrix must register exactly 68 contracts");

        // Reverse direction: no manifest entry is undocumented, apart from
        // the liveness probe and the D-2 non-contract helper.
        for pattern in ROUTE_PATTERNS {
            assert!(
                documented.iter().any(|path| path == pattern)
                    || *pattern == "/health"
                    || *pattern == EXTRA,
                "route {pattern} is missing from the authentication matrix"
            );
        }
    }
}
