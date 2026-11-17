//! Position use cases.

use ica_domain::{Page, Position};

use crate::error::ApplicationError;
use crate::ports::{PositionFilter, PositionRepository};

/// A validated ``GET /api/positions`` request.
///
/// Query validation (``limit`` in 1..=500, ``offset >= 0``, ``search`` at most
/// 200 characters) happens in the HTTP layer, which is where the baseline puts
/// it; by the time a query reaches here every field is already in range.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListPositions {
    pub include_archived: bool,
    pub search: Option<String>,
    pub offset: usize,
    pub limit: usize,
}

/// List one page of positions.
pub async fn list_positions(
    repository: &dyn PositionRepository,
    query: ListPositions,
) -> Result<Page<Position>, ApplicationError> {
    let filter = PositionFilter {
        include_archived: query.include_archived,
        search: query.search,
    };
    let rows = repository
        .list(filter)
        .await
        .map_err(|error| ApplicationError::Unavailable {
            message: error.message().to_string(),
        })?;
    Ok(Page::from_items(rows, query.offset, query.limit))
}

/// Fetch one position or report it missing with the baseline's message.
pub async fn get_position(
    repository: &dyn PositionRepository,
    id: &str,
) -> Result<Position, ApplicationError> {
    match repository.find(id).await {
        Ok(Some(position)) => Ok(position),
        Ok(None) => Err(ApplicationError::not_found("Position", id)),
        Err(error) => Err(ApplicationError::Unavailable {
            message: error.message().to_string(),
        }),
    }
}
