//! Position use cases.

use ica_domain::{Page, Position};

use crate::error::ApplicationError;
use crate::payload::{NewPosition, PositionPatch};
use crate::ports::{PositionFilter, PositionRepository, RepositoryError};
use crate::Scope;

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

fn unavailable(error: RepositoryError) -> ApplicationError {
    error.application_error()
}

/// List one page of positions.
///
/// `total` is the real filtered count (before the window) and the items are the
/// requested slice in the same order the unpaged list would produce them.
pub async fn list_positions(
    repository: &dyn PositionRepository,
    query: ListPositions,
) -> Result<Page<Position>, ApplicationError> {
    let filter = PositionFilter {
        include_archived: query.include_archived,
        search: query.search,
        scope: None,
    };
    let (items, total) = repository
        .page(filter, query.offset, query.limit)
        .await
        .map_err(unavailable)?;
    Ok(Page {
        items,
        total,
        offset: query.offset,
        limit: query.limit,
    })
}

/// SEC-04-B/C: listing with an authenticated scope. A `None` scope keeps the
/// documented Local-mode behaviour; `Some` forces tenant/owner filtering in
/// the repository adapter.
pub async fn list_positions_scoped(
    repository: &dyn PositionRepository,
    query: ListPositions,
    scope: Option<Scope>,
) -> Result<Page<Position>, ApplicationError> {
    let filter = PositionFilter {
        include_archived: query.include_archived,
        search: query.search,
        scope,
    };
    let (items, total) = repository
        .page(filter, query.offset, query.limit)
        .await
        .map_err(unavailable)?;
    Ok(Page {
        items,
        total,
        offset: query.offset,
        limit: query.limit,
    })
}

/// SEC-04-B/C: scoped single-row read. An out-of-scope row is indistinguishable
/// from a missing one (404), so it cannot be used as an IDOR oracle.
pub async fn get_position_scoped(
    repository: &dyn PositionRepository,
    id: &str,
    scope: Option<&Scope>,
) -> Result<Position, ApplicationError> {
    let found = match scope {
        Some(scope) => repository.find_scoped(id, scope).await,
        None => repository.find(id).await,
    };
    match found {
        Ok(Some(position)) => Ok(position),
        Ok(None) => Err(ApplicationError::not_found("Position", id)),
        Err(error) => Err(unavailable(error)),
    }
}

/// Fetch one position or report it missing with the baseline's message.
pub async fn get_position(
    repository: &dyn PositionRepository,
    id: &str,
) -> Result<Position, ApplicationError> {
    match repository.find(id).await {
        Ok(Some(position)) => Ok(position),
        Ok(None) => Err(ApplicationError::not_found("Position", id)),
        Err(error) => Err(unavailable(error)),
    }
}

/// Create a position and audit `position.created`.
pub async fn create_position(
    repository: &dyn PositionRepository,
    new: NewPosition,
) -> Result<Position, ApplicationError> {
    repository.create(new).await.map_err(unavailable)
}

/// SEC-04-B/C: create with the authenticated actor's ownership so the row is
/// invisible to other tenants/owners on subsequent scoped reads.
pub async fn create_position_scoped(
    repository: &dyn PositionRepository,
    new: NewPosition,
    scope: Option<&Scope>,
) -> Result<Position, ApplicationError> {
    match scope {
        Some(scope) => repository.create_scoped(new, scope).await,
        None => repository.create(new).await,
    }
    .map_err(unavailable)
}

/// Patch a position and audit `position.updated`.
///
/// An empty patch is legal: the audit row is still written, exactly like the
/// Python service (which commits the unchanged row).
pub async fn update_position(
    repository: &dyn PositionRepository,
    id: &str,
    patch: PositionPatch,
) -> Result<Position, ApplicationError> {
    match repository.update(id, patch).await {
        Ok(Some(position)) => Ok(position),
        Ok(None) => Err(ApplicationError::not_found("Position", id)),
        Err(error) => Err(unavailable(error)),
    }
}

/// Delete a position and audit `position.deleted`.
///
/// Candidates that pointed at it keep their row; the FK is `ON DELETE SET NULL`.
pub async fn delete_position(
    repository: &dyn PositionRepository,
    id: &str,
) -> Result<(), ApplicationError> {
    match repository.delete(id).await {
        Ok(true) => Ok(()),
        Ok(false) => Err(ApplicationError::not_found("Position", id)),
        Err(error) => Err(unavailable(error)),
    }
}

/// SEC-04-B/C: patch only a row inside the caller's scope.
pub async fn update_position_scoped(
    repository: &dyn PositionRepository,
    id: &str,
    patch: PositionPatch,
    scope: Option<&Scope>,
) -> Result<Position, ApplicationError> {
    let found = match scope {
        Some(scope) => repository.update_scoped(id, patch, scope).await,
        None => repository.update(id, patch).await,
    };
    match found {
        Ok(Some(position)) => Ok(position),
        Ok(None) => Err(ApplicationError::not_found("Position", id)),
        Err(error) => Err(unavailable(error)),
    }
}

/// SEC-04-B/C: delete only a row inside the caller's scope.
pub async fn delete_position_scoped(
    repository: &dyn PositionRepository,
    id: &str,
    scope: Option<&Scope>,
) -> Result<(), ApplicationError> {
    let deleted = match scope {
        Some(scope) => repository.delete_scoped(id, scope).await,
        None => repository.delete(id).await,
    };
    match deleted {
        Ok(true) => Ok(()),
        Ok(false) => Err(ApplicationError::not_found("Position", id)),
        Err(error) => Err(unavailable(error)),
    }
}
