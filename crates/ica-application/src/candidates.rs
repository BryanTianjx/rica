//! Candidate use cases.

use ica_domain::{Candidate, Page, Position};

use crate::error::ApplicationError;
use crate::payload::{CandidateFilter, CandidatePatch, Field, NewCandidate};
use crate::ports::{CandidateRepository, PositionRepository, RepositoryError};

/// A validated ``GET /api/candidates`` request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListCandidates {
    pub search: Option<String>,
    pub status: Option<ica_domain::CandidateStatus>,
    pub position_id: Option<String>,
    pub include_archived: bool,
    pub offset: usize,
    pub limit: usize,
}

fn unavailable(error: RepositoryError) -> ApplicationError {
    ApplicationError::Unavailable {
        message: error.message().to_string(),
    }
}

/// List one page of candidates, `updated_at DESC`, with the real total.
pub async fn list_candidates(
    repository: &dyn CandidateRepository,
    query: ListCandidates,
) -> Result<Page<Candidate>, ApplicationError> {
    let filter = CandidateFilter {
        include_archived: query.include_archived,
        search: query.search,
        status: query.status,
        position_id: query.position_id,
    };
    let (items, total) = repository
        .list(filter, query.offset, query.limit)
        .await
        .map_err(unavailable)?;
    Ok(Page {
        items,
        total,
        offset: query.offset,
        limit: query.limit,
    })
}

/// Fetch one candidate or report it missing with the baseline's message.
pub async fn get_candidate(
    repository: &dyn CandidateRepository,
    id: &str,
) -> Result<Candidate, ApplicationError> {
    match repository.find(id).await {
        Ok(Some(candidate)) => Ok(candidate),
        Ok(None) => Err(ApplicationError::not_found("Candidate", id)),
        Err(error) => Err(unavailable(error)),
    }
}

/// A candidate plus the position object the detail page renders.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateDetail {
    pub candidate: Candidate,
    pub position: Option<Position>,
}

/// Fetch the detail projection.
///
/// The nested position is a *mapped* position, so its `candidate_count` is the
/// real count (mapping the ORM object directly is what left it at 0 in the
/// baseline's first draft).
pub async fn get_candidate_detail(
    repository: &dyn CandidateRepository,
    id: &str,
) -> Result<CandidateDetail, ApplicationError> {
    let candidate = get_candidate(repository, id).await?;
    let position = repository
        .position_of(&candidate.id)
        .await
        .map_err(unavailable)?;
    Ok(CandidateDetail {
        candidate,
        position,
    })
}

/// Create a candidate, validating the optional position link first.
pub async fn create_candidate(
    positions: &dyn PositionRepository,
    repository: &dyn CandidateRepository,
    new: NewCandidate,
) -> Result<Candidate, ApplicationError> {
    if let Some(position_id) = new.position_id.as_deref() {
        crate::positions::get_position(positions, position_id).await?;
    }
    repository.create(new).await.map_err(unavailable)
}

/// Patch a candidate.
///
/// The candidate is resolved before the position check so a request that names
/// both a missing candidate and a missing position reports the candidate —
/// the same order the Python service uses.
pub async fn update_candidate(
    positions: &dyn PositionRepository,
    repository: &dyn CandidateRepository,
    id: &str,
    patch: CandidatePatch,
) -> Result<Candidate, ApplicationError> {
    get_candidate(repository, id).await?;
    if let Field::Value(position_id) = &patch.position_id {
        crate::positions::get_position(positions, position_id).await?;
    }
    match repository.update(id, patch).await {
        Ok(Some(candidate)) => Ok(candidate),
        Ok(None) => Err(ApplicationError::not_found("Candidate", id)),
        Err(error) => Err(unavailable(error)),
    }
}

/// Delete a candidate and every dependent record (FK cascade).
pub async fn delete_candidate(
    repository: &dyn CandidateRepository,
    id: &str,
) -> Result<(), ApplicationError> {
    match repository.delete(id).await {
        Ok(true) => Ok(()),
        Ok(false) => Err(ApplicationError::not_found("Candidate", id)),
        Err(error) => Err(unavailable(error)),
    }
}
