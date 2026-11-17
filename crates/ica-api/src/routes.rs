//! Route table.
//!
//! R0 exposes exactly the three read-only endpoints the card authorises; the
//! remaining 65 baseline routes are mapped (and scheduled) in
//! `docs/rust-migration-contract.md` but not served yet.

use axum::extract::{Path, Query, State};
use axum::routing::get;
use axum::{Json, Router};

use crate::dto::{HealthResponse, PageResponse, PositionRead};
use crate::error::ApiError;
use crate::query::{self, RawListPositionsQuery};
use crate::state::AppState;

/// Build the router for `state`.
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/api/positions", get(list_positions))
        .route("/api/positions/{position_id}", get(get_position))
        .fallback(unknown_path)
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

/// `GET /api/positions` — one page of positions, newest `created_at` first.
///
/// `total` counts every row that matched the filter (before the window), so the
/// UI can render "1-50 of 213" and decide whether to show a next-page control.
async fn list_positions(
    State(state): State<AppState>,
    Query(raw): Query<RawListPositionsQuery>,
) -> Result<Json<PageResponse<PositionRead>>, ApiError> {
    let query = query::validate(raw)?;
    let page = ica_application::list_positions(state.repository(), query).await?;
    let items = page.items.into_iter().map(PositionRead::from).collect();
    Ok(Json(PageResponse::new(
        items,
        page.total,
        page.offset,
        page.limit,
    )))
}

/// `GET /api/positions/{position_id}` — one position, archived or not.
async fn get_position(
    State(state): State<AppState>,
    Path(position_id): Path<String>,
) -> Result<Json<PositionRead>, ApiError> {
    let position = ica_application::get_position(state.repository(), &position_id).await?;
    Ok(Json(PositionRead::from(position)))
}

/// Unknown path: keep the project's error envelope instead of axum's plain
/// text 404, so a client never has to parse two error formats.
async fn unknown_path() -> ApiError {
    ApiError::unknown_path()
}
