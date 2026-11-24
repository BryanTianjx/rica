//! Manual send-record use cases.

use std::collections::BTreeMap;

use ica_domain::{Page, SendRecord};

use crate::error::ApplicationError;
use crate::payload::{NewSendRecord, SendRecordFilter};
use crate::ports::{CandidateRepository, RepositoryError, SendRecordRepository};

/// Statuses a draft may be in when its text is reported as sent.
///
/// Approval is what makes the wording the human's own; `copied` is the last
/// step before pasting into the channel. A rejected draft can never have been
/// sent.
const SENDABLE_DRAFT_STATUSES: [&str; 2] = ["approved", "copied"];

/// A validated ``GET /api/send-records`` request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListSendRecords {
    pub candidate_id: Option<String>,
    pub task_id: Option<String>,
    pub draft_id: Option<String>,
    pub status: Option<ica_domain::ResultType>,
    pub offset: usize,
    pub limit: usize,
}

fn unavailable(error: RepositoryError) -> ApplicationError {
    error.application_error()
}

/// Send records, newest send first (`sent_at DESC, created_at DESC`).
pub async fn list_send_records(
    repository: &dyn SendRecordRepository,
    query: ListSendRecords,
) -> Result<Page<SendRecord>, ApplicationError> {
    list_send_records_scoped(repository, query, None).await
}

/// SEC-04: list with the authenticated tenant/owner predicate in SQL.
pub async fn list_send_records_scoped(
    repository: &dyn SendRecordRepository,
    query: ListSendRecords,
    scope: Option<&crate::Scope>,
) -> Result<Page<SendRecord>, ApplicationError> {
    let filter = SendRecordFilter {
        candidate_id: query.candidate_id,
        task_id: query.task_id,
        draft_id: query.draft_id,
        status: query.status,
        scope: scope.cloned(),
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

/// Append one manual send record and its `send.recorded` audit event.
///
/// The candidate is the anchor: an optional task and draft must belong to the
/// same candidate, and a linked draft must already be approved or copied, or
/// the UI could claim the human review step was skipped. Those are 409
/// `business_rule_violation` responses with the identifiers in `details`.
pub async fn record_send(
    candidates: &dyn CandidateRepository,
    repository: &dyn SendRecordRepository,
    new: NewSendRecord,
) -> Result<SendRecord, ApplicationError> {
    record_send_scoped(candidates, repository, new, None).await
}

/// SEC-04: the candidate anchor and both optional children (task, draft) are
/// resolved inside the caller's scope, so a foreign id is reported as missing.
pub async fn record_send_scoped(
    candidates: &dyn CandidateRepository,
    repository: &dyn SendRecordRepository,
    new: NewSendRecord,
    scope: Option<&crate::Scope>,
) -> Result<SendRecord, ApplicationError> {
    let candidate =
        crate::candidates::get_candidate_scoped(candidates, &new.candidate_id, scope).await?;

    if let Some(task_id) = new.task_id.as_deref() {
        let owner = match scope {
            Some(scope) => repository.follow_up_candidate_scoped(task_id, scope).await,
            None => repository.follow_up_candidate(task_id).await,
        }
        .map_err(unavailable)?
        .ok_or_else(|| ApplicationError::not_found("Follow-up task", task_id))?;
        if owner != candidate.id {
            let mut details = BTreeMap::new();
            details.insert("task_id".to_string(), task_id.to_string());
            details.insert("candidate_id".to_string(), candidate.id.clone());
            return Err(ApplicationError::conflict(
                "The follow-up task belongs to a different candidate",
                details,
            ));
        }
    }

    if let Some(draft_id) = new.draft_id.as_deref() {
        let found = match scope {
            Some(scope) => {
                repository
                    .draft_candidate_status_scoped(draft_id, scope)
                    .await
            }
            None => repository.draft_candidate_status(draft_id).await,
        }
        .map_err(unavailable)?;
        let (owner, status) =
            found.ok_or_else(|| ApplicationError::not_found("Draft", draft_id))?;
        if owner != candidate.id {
            let mut details = BTreeMap::new();
            details.insert("draft_id".to_string(), draft_id.to_string());
            details.insert("candidate_id".to_string(), candidate.id.clone());
            return Err(ApplicationError::conflict(
                "The draft belongs to a different candidate",
                details,
            ));
        }
        if !SENDABLE_DRAFT_STATUSES.contains(&status.as_str()) {
            let mut details = BTreeMap::new();
            details.insert("draft_id".to_string(), draft_id.to_string());
            details.insert("draft_status".to_string(), status);
            return Err(ApplicationError::conflict(
                "Only an approved or copied draft can be recorded as sent",
                details,
            ));
        }
    }

    match scope {
        Some(scope) => repository.create_scoped(new, scope).await,
        None => repository.create(new).await,
    }
    .map_err(unavailable)
}
