//! Dashboard use case: aggregated counts plus the maintenance tick the
//! baseline runs on the same read path.
//!
//! Every number is computed from the actual database; the sweeps
//! (`expire_old_candidates`, audit pruning, import raw-text retention) are
//! idempotent and run *before* the counts, exactly like
//! `run_maintenance` in the baseline.

use chrono::{Duration, Utc};
use ica_domain::{CandidateStatus, FollowUpTaskStatus, Message};

use crate::error::ApplicationError;
use crate::follow_ups::{DATA_RETENTION_DAYS, RAW_TEXT_TTL_DAYS};
use crate::ports::{DashboardRepository, RepositoryError};

/// The states whose tasks the workbench section counts and lists.
pub const PENDING_STATUSES: [FollowUpTaskStatus; 4] = [
    FollowUpTaskStatus::Pending,
    FollowUpTaskStatus::Eligible,
    FollowUpTaskStatus::Drafting,
    FollowUpTaskStatus::Reviewing,
];

/// The draft statuses the pending-draft counter sees (lower-case column
/// values, exactly the baseline's `["generated", "edited"]`).
const PENDING_DRAFT_STATUSES: [&str; 2] = ["generated", "edited"];

/// Everything `GET /api/dashboard` answers, before serialisation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DashboardSnapshot {
    pub candidate_total: i64,
    /// `(status, count)` pairs — the API fills the missing enumeration keys
    /// with 0, the way `status_counts.setdefault(status.value, 0)` does.
    pub candidate_status_counts: Vec<(String, i64)>,
    pub active_follow_up_count: i64,
    pub pending_follow_ups: Vec<ica_domain::FollowUpTask>,
    pub awaiting_result_count: i64,
    pub pending_draft_count: i64,
    pub recent_messages: Vec<Message>,
    pub position_total: i64,
    pub open_position_count: i64,
}

/// Aggregate the recruiter's current state.
///
/// Deliberately **no** task sweep here: the baseline expires stale tasks on
/// the *list* endpoint only, and the dashboard must not write what the
/// baseline would not write on this read path.
pub async fn dashboard(
    dashboard: &dyn DashboardRepository,
) -> Result<DashboardSnapshot, ApplicationError> {
    run_maintenance(dashboard).await?;

    let candidate_total = dashboard.candidate_total().await.map_err(unavailable)?;
    let candidate_status_counts = dashboard
        .candidate_status_counts()
        .await
        .map_err(unavailable)?;
    let pending_follow_ups = dashboard
        .pending_tasks(&PENDING_STATUSES, 20)
        .await
        .map_err(unavailable)?;
    let active_follow_up_count = dashboard
        .task_count_in(&PENDING_STATUSES)
        .await
        .map_err(unavailable)?;
    let awaiting_result_count = dashboard
        .task_count_in(&[FollowUpTaskStatus::AwaitingResult])
        .await
        .map_err(unavailable)?;
    let pending_draft_count = dashboard
        .draft_count_in(&PENDING_DRAFT_STATUSES)
        .await
        .map_err(unavailable)?;
    let recent_messages = dashboard.recent_messages(10).await.map_err(unavailable)?;
    let position_total = dashboard.position_total().await.map_err(unavailable)?;
    let open_position_count = dashboard.open_position_count().await.map_err(unavailable)?;

    Ok(DashboardSnapshot {
        candidate_total,
        candidate_status_counts,
        active_follow_up_count,
        pending_follow_ups,
        awaiting_result_count,
        pending_draft_count,
        recent_messages,
        position_total,
        open_position_count,
    })
}

/// The maintenance tick: candidate expiry (hard delete, retention window),
/// audit-trail pruning, orphaned import raw-text blanking.
async fn run_maintenance(dashboard: &dyn DashboardRepository) -> Result<(), ApplicationError> {
    let now = Utc::now();
    if DATA_RETENTION_DAYS > 0 {
        let cutoff = now - Duration::days(DATA_RETENTION_DAYS);
        dashboard
            .expire_old_candidates(cutoff)
            .await
            .map_err(unavailable)?;
        dashboard
            .prune_audit_events(cutoff)
            .await
            .map_err(unavailable)?;
    }
    if RAW_TEXT_TTL_DAYS > 0 {
        dashboard
            .prune_stale_import_raw_text(now - Duration::days(RAW_TEXT_TTL_DAYS))
            .await
            .map_err(unavailable)?;
    }
    Ok(())
}

fn unavailable(error: RepositoryError) -> ApplicationError {
    error.application_error()
}

/// The nine candidate statuses, in enumeration order — the dashboard's status
/// map always carries all nine keys (missing groups render as 0).
pub const CANDIDATE_STATUS_KEYS: [CandidateStatus; 9] = [
    CandidateStatus::New,
    CandidateStatus::Contacted,
    CandidateStatus::ReadNoReply,
    CandidateStatus::Replied,
    CandidateStatus::Active,
    CandidateStatus::Interview,
    CandidateStatus::Rejected,
    CandidateStatus::Withdrawn,
    CandidateStatus::Closed,
];
