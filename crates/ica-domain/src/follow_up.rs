//! Follow-up domain rules — a pure port of ``app/domain/follow_up.py``.
//!
//! The 12-state machine, its transition table and the policy engine are the
//! contract: the wire behaviour (which transitions 409 with which message) is
//! derived from these rules, measured against the Python baseline in
//! `docs/contract-baseline/w2-python-facts.md` §1. The semantic red line is
//! preserved verbatim: `Approved` only means the draft was approved, `Copied`
//! only means the text was copied, `AwaitingResult` only means the user still
//! owes a result — **none of them ever implies the message was sent**.

use std::collections::VecDeque;

use chrono::{DateTime, Utc};

use crate::candidate::CandidateStatus;
use crate::position::PositionStatus;

/// The follow-up task state machine's twelve states (case-sensitive strings).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FollowUpTaskStatus {
    Pending,
    Eligible,
    Drafting,
    Reviewing,
    Rejected,
    Approved,
    Copied,
    AwaitingResult,
    Completed,
    Cancelled,
    Expired,
    Blocked,
}

impl FollowUpTaskStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "Pending",
            Self::Eligible => "Eligible",
            Self::Drafting => "Drafting",
            Self::Reviewing => "Reviewing",
            Self::Rejected => "Rejected",
            Self::Approved => "Approved",
            Self::Copied => "Copied",
            Self::AwaitingResult => "AwaitingResult",
            Self::Completed => "Completed",
            Self::Cancelled => "Cancelled",
            Self::Expired => "Expired",
            Self::Blocked => "Blocked",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        Some(match raw {
            "Pending" => Self::Pending,
            "Eligible" => Self::Eligible,
            "Drafting" => Self::Drafting,
            "Reviewing" => Self::Reviewing,
            "Rejected" => Self::Rejected,
            "Approved" => Self::Approved,
            "Copied" => Self::Copied,
            "AwaitingResult" => Self::AwaitingResult,
            "Completed" => Self::Completed,
            "Cancelled" => Self::Cancelled,
            "Expired" => Self::Expired,
            "Blocked" => Self::Blocked,
            _ => return None,
        })
    }

    /// No further transitions allowed from this state.
    pub fn is_closed(self) -> bool {
        matches!(self, Self::Completed | Self::Cancelled | Self::Expired)
    }

    /// States that imply a follow-up attempt was actually made. Only these
    /// count toward the max-active-follow-up policy — drafting and approving a
    /// message do not, because the requirement bases follow-up counts on
    /// confirmed communication records, not drafts.
    pub fn means_contacted(self) -> bool {
        matches!(self, Self::Copied | Self::AwaitingResult | Self::Completed)
    }

    /// Whether the user may set this state by hand. The semantic states are
    /// owned by the flows that record them: `Approved` by the draft-approval
    /// flow, `Copied` by the copy flow, `Completed` by result recording,
    /// `Expired` by the sweep and `Blocked` by the policy evaluation.
    pub fn is_manual_settable(self) -> bool {
        matches!(
            self,
            Self::Pending
                | Self::Eligible
                | Self::Drafting
                | Self::Reviewing
                | Self::Rejected
                | Self::Cancelled
        )
    }

    /// Whether a recorded result may close this state out. Terminal states and
    /// `Blocked` are excluded: a closed task stays closed, and a blocked task
    /// must not be completed behind the blocking condition.
    pub fn is_result_completable(self) -> bool {
        matches!(
            self,
            Self::Pending
                | Self::Eligible
                | Self::Drafting
                | Self::Reviewing
                | Self::Approved
                | Self::Copied
                | Self::AwaitingResult
        )
    }
}

/// The legal transitions, one row per source state (`app/domain/follow_up.py`
/// `ALLOWED_TRANSITIONS`, ported verbatim).
const ALLOWED_TRANSITIONS: &[(FollowUpTaskStatus, &[FollowUpTaskStatus])] = &[
    (
        FollowUpTaskStatus::Pending,
        &[
            FollowUpTaskStatus::Eligible,
            FollowUpTaskStatus::Cancelled,
            FollowUpTaskStatus::Expired,
            FollowUpTaskStatus::Blocked,
        ],
    ),
    (
        FollowUpTaskStatus::Eligible,
        &[
            FollowUpTaskStatus::Drafting,
            FollowUpTaskStatus::Cancelled,
            FollowUpTaskStatus::Expired,
            FollowUpTaskStatus::Blocked,
        ],
    ),
    (
        FollowUpTaskStatus::Drafting,
        &[
            FollowUpTaskStatus::Reviewing,
            FollowUpTaskStatus::Rejected,
            FollowUpTaskStatus::Cancelled,
            FollowUpTaskStatus::Expired,
            FollowUpTaskStatus::Blocked,
        ],
    ),
    (
        FollowUpTaskStatus::Reviewing,
        &[
            // Copying without a prior approval is legitimate: the user may copy
            // a draft they never explicitly approved. The task then passes
            // through `Copied` without ever claiming it was `Approved`.
            FollowUpTaskStatus::Approved,
            FollowUpTaskStatus::Rejected,
            FollowUpTaskStatus::Drafting,
            FollowUpTaskStatus::Copied,
            FollowUpTaskStatus::Cancelled,
            FollowUpTaskStatus::Expired,
            FollowUpTaskStatus::Blocked,
        ],
    ),
    (
        FollowUpTaskStatus::Rejected,
        &[FollowUpTaskStatus::Cancelled, FollowUpTaskStatus::Expired],
    ),
    (
        FollowUpTaskStatus::Approved,
        &[
            FollowUpTaskStatus::Copied,
            FollowUpTaskStatus::Cancelled,
            FollowUpTaskStatus::Expired,
        ],
    ),
    (
        FollowUpTaskStatus::Copied,
        &[
            FollowUpTaskStatus::AwaitingResult,
            FollowUpTaskStatus::Completed,
            FollowUpTaskStatus::Cancelled,
        ],
    ),
    (
        FollowUpTaskStatus::AwaitingResult,
        &[FollowUpTaskStatus::Completed, FollowUpTaskStatus::Cancelled],
    ),
    (FollowUpTaskStatus::Completed, &[]),
    (FollowUpTaskStatus::Cancelled, &[]),
    (FollowUpTaskStatus::Expired, &[]),
    (
        FollowUpTaskStatus::Blocked,
        &[FollowUpTaskStatus::Cancelled, FollowUpTaskStatus::Expired],
    ),
];

/// Whether `current -> target` is a legal transition. A self-transition is
/// always legal (measured: re-cancelling a `Cancelled` task answers 200).
pub fn can_transition(current: FollowUpTaskStatus, target: FollowUpTaskStatus) -> bool {
    if current == target {
        return true;
    }
    ALLOWED_TRANSITIONS
        .iter()
        .any(|(state, targets)| *state == current && targets.contains(&target))
}

/// The shortest legal path from `current` to `target` (breadth-first, so the
/// fewest fabricated intermediate steps), or `None` when unreachable.
pub fn find_transition_path(
    current: FollowUpTaskStatus,
    target: FollowUpTaskStatus,
) -> Option<Vec<FollowUpTaskStatus>> {
    if current == target {
        return Some(Vec::new());
    }
    let mut queue: VecDeque<(FollowUpTaskStatus, Vec<FollowUpTaskStatus>)> =
        VecDeque::from([(current, Vec::new())]);
    let mut seen: Vec<FollowUpTaskStatus> = vec![current];
    while let Some((state, path)) = queue.pop_front() {
        let targets = ALLOWED_TRANSITIONS
            .iter()
            .find(|(from, _)| *from == state)
            .map(|(_, targets)| *targets)
            .unwrap_or(&[]);
        for next in targets {
            if seen.contains(next) {
                continue;
            }
            let mut new_path = path.clone();
            new_path.push(*next);
            if *next == target {
                return Some(new_path);
            }
            seen.push(*next);
            queue.push_back((*next, new_path));
        }
    }
    None
}

/// Whether a candidate in this status blocks ordinary follow-up
/// (`CandidateStatus.is_terminal` in the baseline).
pub fn candidate_blocks_follow_up(status: CandidateStatus) -> bool {
    matches!(
        status,
        CandidateStatus::Rejected | CandidateStatus::Withdrawn | CandidateStatus::Closed
    )
}

/// Configurable follow-up thresholds — application defaults for personal
/// recruiting workflow, not platform rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FollowUpPolicy {
    pub first_follow_up_delay_hours: i64,
    pub max_active_follow_ups: i64,
    pub min_interval_hours: i64,
    pub task_validity_days: i64,
}

impl Default for FollowUpPolicy {
    fn default() -> Self {
        Self {
            first_follow_up_delay_hours: 24,
            max_active_follow_ups: 2,
            min_interval_hours: 48,
            task_validity_days: 7,
        }
    }
}

/// Whether a follow-up is advised.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecisionOutcome {
    Eligible,
    Wait,
    Blocked,
}

impl DecisionOutcome {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Eligible => "eligible",
            Self::Wait => "wait",
            Self::Blocked => "blocked",
        }
    }
}

/// Result of evaluating whether a follow-up should happen now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FollowUpDecision {
    pub outcome: DecisionOutcome,
    pub reasons: Vec<String>,
    pub block_reasons: Vec<String>,
    pub recommended_at: Option<DateTime<Utc>>,
    pub suggested_objective: Option<String>,
    pub risks: Vec<String>,
}

impl FollowUpDecision {
    pub fn eligible(&self) -> bool {
        self.outcome == DecisionOutcome::Eligible
    }
}

/// Inputs the decision engine needs, gathered by the application layer.
#[derive(Debug, Clone)]
pub struct EvaluateInput {
    pub candidate_status: CandidateStatus,
    pub position_status: Option<PositionStatus>,
    pub last_message_at: Option<DateTime<Utc>>,
    pub last_message_from_candidate: Option<bool>,
    pub active_follow_up_count: i64,
    pub last_follow_up_at: Option<DateTime<Utc>>,
    pub now: DateTime<Utc>,
    pub policy: Option<FollowUpPolicy>,
}

/// Decide whether a follow-up is appropriate right now — a deterministic,
/// value-for-value port of `evaluate_follow_up`, including the exact reason
/// wordings the frontend renders.
pub fn evaluate_follow_up(input: EvaluateInput) -> FollowUpDecision {
    let policy = input.policy.unwrap_or_default();
    let mut decision = FollowUpDecision {
        outcome: DecisionOutcome::Wait,
        reasons: Vec::new(),
        block_reasons: Vec::new(),
        recommended_at: None,
        suggested_objective: None,
        risks: Vec::new(),
    };
    let now = input.now;

    // ── Blocking conditions (highest priority) ─────────────────────────────
    if candidate_blocks_follow_up(input.candidate_status) {
        decision.outcome = DecisionOutcome::Blocked;
        decision.block_reasons.push(format!(
            "Candidate status is {}; ordinary follow-up is not allowed.",
            input.candidate_status.as_str()
        ));
        return decision;
    }
    if let Some(position_status) = input.position_status {
        if position_status.blocks_follow_up() {
            decision.outcome = DecisionOutcome::Blocked;
            decision.block_reasons.push(format!(
                "Position status is {}; ordinary follow-up is not allowed.",
                position_status.as_str()
            ));
            return decision;
        }
    }
    if input.candidate_status == CandidateStatus::Interview {
        decision.outcome = DecisionOutcome::Blocked;
        decision.block_reasons.push(
            "Candidate entered the interview stage; use interview follow-up instead.".to_string(),
        );
        return decision;
    }

    // ── Count limits ───────────────────────────────────────────────────────
    if input.active_follow_up_count >= policy.max_active_follow_ups {
        decision.outcome = DecisionOutcome::Blocked;
        decision.block_reasons.push(format!(
            "Active follow-up count {} reached the limit of {}.",
            input.active_follow_up_count, policy.max_active_follow_ups
        ));
        return decision;
    }

    // ── Timing ─────────────────────────────────────────────────────────────
    let Some(last_message_at) = input.last_message_at else {
        decision.outcome = DecisionOutcome::Eligible;
        decision
            .reasons
            .push("No prior communication recorded.".to_string());
        decision.suggested_objective = Some("greeting".to_string());
        return decision;
    };

    if input.last_message_from_candidate == Some(true) {
        decision.outcome = DecisionOutcome::Eligible;
        decision
            .reasons
            .push("The last message came from the candidate; a reply is expected.".to_string());
        decision.suggested_objective = Some("reply".to_string());
        return decision;
    }

    // Last message was ours — a follow-up may be due.
    let elapsed = now - last_message_at;
    if input.active_follow_up_count == 0 {
        let delay = chrono::Duration::hours(policy.first_follow_up_delay_hours);
        if elapsed >= delay {
            decision.outcome = DecisionOutcome::Eligible;
            decision.reasons.push(format!(
                "{} elapsed since the last message, meeting the {}h first-follow-up wait.",
                humanize(elapsed),
                policy.first_follow_up_delay_hours
            ));
            decision.suggested_objective = Some("polite_follow_up".to_string());
        } else {
            decision.recommended_at = Some(last_message_at + delay);
            decision.reasons.push(format!(
                "Waiting for the first follow-up delay of {}h.",
                policy.first_follow_up_delay_hours
            ));
        }
        return decision;
    }

    // Subsequent follow-ups need the minimum interval.
    let min_interval = chrono::Duration::hours(policy.min_interval_hours);
    let reference = input.last_follow_up_at.unwrap_or(last_message_at);
    if now - reference >= min_interval {
        decision.outcome = DecisionOutcome::Eligible;
        decision.reasons.push(format!(
            "At least {}h since the last follow-up attempt.",
            policy.min_interval_hours
        ));
        decision.suggested_objective = Some("polite_follow_up".to_string());
    } else {
        decision.recommended_at = Some(reference + min_interval);
        decision.reasons.push(format!(
            "Waiting for the {}h minimum interval between follow-ups.",
            policy.min_interval_hours
        ));
    }
    decision
}

/// Absolute expiry time of an ordinary follow-up task.
pub fn task_expiry(
    task_created_at: DateTime<Utc>,
    policy: Option<FollowUpPolicy>,
) -> DateTime<Utc> {
    task_created_at + chrono::Duration::days(policy.unwrap_or_default().task_validity_days)
}

/// Whether an ordinary task has outlived its validity window.
pub fn is_task_expired(
    task_created_at: DateTime<Utc>,
    now: DateTime<Utc>,
    policy: Option<FollowUpPolicy>,
) -> bool {
    now >= task_expiry(task_created_at, policy)
}

/// `f"{total_hours // 24}d{total_hours % 24}h"` below a day, plain hours
/// otherwise (`_humanize` in the baseline).
fn humanize(delta: chrono::Duration) -> String {
    let total_hours = delta.num_hours();
    if total_hours < 24 {
        return format!("{total_hours}h");
    }
    format!("{}d{}h", total_hours / 24, total_hours % 24)
}

/// One follow-up task — the `follow_up_tasks` table's wire-relevant fields,
/// plus the read-model join field `candidate_name` (the baseline's mapper adds
/// `task.candidate.name`; the same derived-read-field pattern `Candidate`
/// carries for `position_name` and its counters).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FollowUpTask {
    pub id: String,
    pub candidate_id: String,
    pub position_id: Option<String>,
    pub status: FollowUpTaskStatus,
    pub objective: Option<String>,
    pub reason: Option<String>,
    pub block_reason: Option<String>,
    pub recommended_at: Option<DateTime<Utc>>,
    pub expires_at: Option<DateTime<Utc>>,
    pub last_follow_up_at: Option<DateTime<Utc>>,
    pub follow_up_count: i64,
    pub completed_at: Option<DateTime<Utc>>,
    pub result_type: Option<String>,
    pub result_note: Option<String>,
    pub result_recorded_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub candidate_name: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn at(hours: i64) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, 10, 9, 0, 0).unwrap() + chrono::Duration::hours(hours)
    }

    #[test]
    fn the_twelve_statuses_round_trip_their_exact_strings() {
        for raw in [
            "Pending",
            "Eligible",
            "Drafting",
            "Reviewing",
            "Rejected",
            "Approved",
            "Copied",
            "AwaitingResult",
            "Completed",
            "Cancelled",
            "Expired",
            "Blocked",
        ] {
            assert_eq!(FollowUpTaskStatus::parse(raw).unwrap().as_str(), raw);
        }
        // Case-sensitive, exactly like the Python enum.
        assert!(FollowUpTaskStatus::parse("pending").is_none());
        assert!(FollowUpTaskStatus::parse("awaiting_result").is_none());
    }

    #[test]
    fn only_three_states_are_closed() {
        for status in [
            FollowUpTaskStatus::Completed,
            FollowUpTaskStatus::Cancelled,
            FollowUpTaskStatus::Expired,
        ] {
            assert!(status.is_closed(), "{}", status.as_str());
        }
        assert!(!FollowUpTaskStatus::Blocked.is_closed());
        assert!(!FollowUpTaskStatus::Copied.is_closed());
    }

    #[test]
    fn means_contacted_never_includes_draft_or_approval_states() {
        // The red line: drafting/approving never implies a message was sent,
        // so those states must not count as contact attempts.
        assert!(FollowUpTaskStatus::Copied.means_contacted());
        assert!(FollowUpTaskStatus::AwaitingResult.means_contacted());
        assert!(FollowUpTaskStatus::Completed.means_contacted());
        for status in [
            FollowUpTaskStatus::Pending,
            FollowUpTaskStatus::Eligible,
            FollowUpTaskStatus::Drafting,
            FollowUpTaskStatus::Reviewing,
            FollowUpTaskStatus::Rejected,
            FollowUpTaskStatus::Approved,
            FollowUpTaskStatus::Expired,
            FollowUpTaskStatus::Blocked,
            FollowUpTaskStatus::Cancelled,
        ] {
            assert!(!status.means_contacted(), "{}", status.as_str());
        }
    }

    #[test]
    fn self_transitions_are_always_legal() {
        for current in [
            FollowUpTaskStatus::Pending,
            FollowUpTaskStatus::Completed,
            FollowUpTaskStatus::Expired,
            FollowUpTaskStatus::Cancelled,
        ] {
            assert!(can_transition(current, current));
        }
    }

    #[test]
    fn the_measured_matrix_holds() {
        // A few load-bearing cells from the 109-row live probe
        // (docs/contract-baseline/w2-python-facts.md §1.1).
        assert!(can_transition(
            FollowUpTaskStatus::Pending,
            FollowUpTaskStatus::Eligible
        ));
        assert!(!can_transition(
            FollowUpTaskStatus::Pending,
            FollowUpTaskStatus::Drafting
        ));
        assert!(can_transition(
            FollowUpTaskStatus::Reviewing,
            FollowUpTaskStatus::Copied
        ));
        assert!(!can_transition(
            FollowUpTaskStatus::Rejected,
            FollowUpTaskStatus::Drafting
        ));
        assert!(can_transition(
            FollowUpTaskStatus::Blocked,
            FollowUpTaskStatus::Cancelled
        ));
        assert!(!can_transition(
            FollowUpTaskStatus::Expired,
            FollowUpTaskStatus::Cancelled
        ));
        assert!(can_transition(
            FollowUpTaskStatus::AwaitingResult,
            FollowUpTaskStatus::Completed
        ));
        assert!(can_transition(
            FollowUpTaskStatus::Approved,
            FollowUpTaskStatus::Cancelled
        ));
        // The full table: every (current, target) pair.
        for (current, targets) in ALLOWED_TRANSITIONS {
            for target in *targets {
                assert!(can_transition(*current, *target));
            }
            for other in [
                FollowUpTaskStatus::Pending,
                FollowUpTaskStatus::Eligible,
                FollowUpTaskStatus::Drafting,
                FollowUpTaskStatus::Reviewing,
                FollowUpTaskStatus::Rejected,
                FollowUpTaskStatus::Approved,
                FollowUpTaskStatus::Copied,
                FollowUpTaskStatus::AwaitingResult,
                FollowUpTaskStatus::Completed,
                FollowUpTaskStatus::Cancelled,
                FollowUpTaskStatus::Expired,
                FollowUpTaskStatus::Blocked,
            ] {
                if targets.contains(&other) || *current == other {
                    continue;
                }
                assert!(!can_transition(*current, other), "{current:?} -> {other:?}");
            }
        }
    }

    #[test]
    fn paths_walk_the_shortest_route() {
        let path =
            find_transition_path(FollowUpTaskStatus::Eligible, FollowUpTaskStatus::Completed)
                .unwrap();
        // Eligible -> Drafting -> Reviewing -> Copied -> Completed: BFS finds
        // the 4-step route through `Copied` (whose table lists `Completed`
        // directly) before the longer one via `AwaitingResult`.
        assert_eq!(
            path,
            vec![
                FollowUpTaskStatus::Drafting,
                FollowUpTaskStatus::Reviewing,
                FollowUpTaskStatus::Copied,
                FollowUpTaskStatus::Completed,
            ]
        );
        assert_eq!(
            find_transition_path(FollowUpTaskStatus::Completed, FollowUpTaskStatus::Pending),
            None
        );
        assert_eq!(
            find_transition_path(FollowUpTaskStatus::Pending, FollowUpTaskStatus::Pending),
            Some(vec![])
        );
    }

    #[test]
    fn manual_settable_and_result_completable_match_the_baseline() {
        for status in [
            FollowUpTaskStatus::Pending,
            FollowUpTaskStatus::Eligible,
            FollowUpTaskStatus::Drafting,
            FollowUpTaskStatus::Reviewing,
            FollowUpTaskStatus::Rejected,
            FollowUpTaskStatus::Cancelled,
        ] {
            assert!(status.is_manual_settable());
        }
        for status in [
            FollowUpTaskStatus::Approved,
            FollowUpTaskStatus::Copied,
            FollowUpTaskStatus::AwaitingResult,
            FollowUpTaskStatus::Completed,
            FollowUpTaskStatus::Expired,
            FollowUpTaskStatus::Blocked,
        ] {
            assert!(!status.is_manual_settable());
        }
        assert!(FollowUpTaskStatus::Approved.is_result_completable());
        assert!(!FollowUpTaskStatus::Rejected.is_result_completable());
        assert!(!FollowUpTaskStatus::Blocked.is_result_completable());
    }

    #[test]
    fn evaluation_blocks_come_first_in_priority_order() {
        let input = |status: CandidateStatus, position: Option<PositionStatus>| EvaluateInput {
            candidate_status: status,
            position_status: position,
            last_message_at: None,
            last_message_from_candidate: None,
            active_follow_up_count: 0,
            last_follow_up_at: None,
            now: at(0),
            policy: None,
        };
        let decision = evaluate_follow_up(input(CandidateStatus::Rejected, None));
        assert_eq!(decision.outcome, DecisionOutcome::Blocked);
        assert_eq!(
            decision.block_reasons,
            vec!["Candidate status is Rejected; ordinary follow-up is not allowed."]
        );
        let decision =
            evaluate_follow_up(input(CandidateStatus::Active, Some(PositionStatus::Closed)));
        assert_eq!(
            decision.block_reasons,
            vec!["Position status is Closed; ordinary follow-up is not allowed."]
        );
        let decision = evaluate_follow_up(input(CandidateStatus::Interview, None));
        assert_eq!(
            decision.block_reasons,
            vec!["Candidate entered the interview stage; use interview follow-up instead."]
        );
    }

    #[test]
    fn evaluation_hits_the_timing_branches() {
        let input = |last: Option<DateTime<Utc>>,
                     from_candidate: Option<bool>,
                     count: i64,
                     last_follow_up: Option<DateTime<Utc>>| EvaluateInput {
            candidate_status: CandidateStatus::Active,
            position_status: Some(PositionStatus::Open),
            last_message_at: last,
            last_message_from_candidate: from_candidate,
            active_follow_up_count: count,
            last_follow_up_at: last_follow_up,
            now: at(100),
            policy: None,
        };
        // No message at all.
        let decision = evaluate_follow_up(input(None, None, 0, None));
        assert_eq!(decision.outcome, DecisionOutcome::Eligible);
        assert_eq!(decision.reasons, vec!["No prior communication recorded."]);
        assert_eq!(decision.suggested_objective.as_deref(), Some("greeting"));
        // Candidate spoke last.
        let decision = evaluate_follow_up(input(Some(at(99)), Some(true), 0, None));
        assert_eq!(
            decision.reasons,
            vec!["The last message came from the candidate; a reply is expected."]
        );
        assert_eq!(decision.suggested_objective.as_deref(), Some("reply"));
        // Ours, first follow-up, inside the delay: WAIT with recommended_at.
        let decision = evaluate_follow_up(input(Some(at(90)), Some(false), 0, None));
        assert_eq!(decision.outcome, DecisionOutcome::Wait);
        assert_eq!(
            decision.reasons,
            vec!["Waiting for the first follow-up delay of 24h."]
        );
        assert_eq!(
            decision.recommended_at,
            Some(at(90) + chrono::Duration::hours(24))
        );
        // Ours, first follow-up, delay met: 40h elapsed, humanized as 1d16h.
        let decision = evaluate_follow_up(input(Some(at(60)), Some(false), 0, None));
        assert_eq!(decision.outcome, DecisionOutcome::Eligible);
        assert_eq!(
            decision.reasons,
            vec!["1d16h elapsed since the last message, meeting the 24h first-follow-up wait."]
        );
        assert_eq!(
            decision.suggested_objective.as_deref(),
            Some("polite_follow_up")
        );
        // Ours, subsequent, interval met (reference: last_follow_up_at).
        let decision = evaluate_follow_up(input(Some(at(10)), Some(false), 1, Some(at(30))));
        assert_eq!(
            decision.reasons,
            vec!["At least 48h since the last follow-up attempt."]
        );
        // Ours, subsequent, interval not met (reference: last_follow_up_at).
        let decision = evaluate_follow_up(input(Some(at(10)), Some(false), 1, Some(at(90))));
        assert_eq!(decision.outcome, DecisionOutcome::Wait);
        assert_eq!(
            decision.reasons,
            vec!["Waiting for the 48h minimum interval between follow-ups."]
        );
        assert_eq!(
            decision.recommended_at,
            Some(at(90) + chrono::Duration::hours(48))
        );
    }

    #[test]
    fn count_limit_blocks_before_timing() {
        let decision = evaluate_follow_up(EvaluateInput {
            candidate_status: CandidateStatus::Active,
            position_status: Some(PositionStatus::Open),
            last_message_at: None,
            last_message_from_candidate: None,
            active_follow_up_count: 2,
            last_follow_up_at: None,
            now: at(0),
            policy: None,
        });
        assert_eq!(decision.outcome, DecisionOutcome::Blocked);
        assert_eq!(
            decision.block_reasons,
            vec!["Active follow-up count 2 reached the limit of 2."]
        );
    }

    #[test]
    fn humanize_matches_the_python_helper() {
        assert_eq!(humanize(chrono::Duration::hours(5)), "5h");
        assert_eq!(humanize(chrono::Duration::hours(23)), "23h");
        assert_eq!(humanize(chrono::Duration::hours(24)), "1d0h");
        assert_eq!(humanize(chrono::Duration::hours(50)), "2d2h");
    }

    #[test]
    fn expiry_is_created_at_plus_the_validity_window() {
        assert_eq!(task_expiry(at(0), None), at(0) + chrono::Duration::days(7));
        assert!(is_task_expired(
            at(0),
            at(0) + chrono::Duration::days(7),
            None
        ));
        assert!(!is_task_expired(
            at(0),
            at(0) + chrono::Duration::days(7) - chrono::Duration::seconds(1),
            None
        ));
    }
}
