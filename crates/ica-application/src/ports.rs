//! Repository ports (the application's outward-facing seam).

use std::collections::BTreeMap;

use crate::ai_registry::{AiModelData, AiUsageRow};
use crate::auth::{ShareData, UserData};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use ica_domain::{
    AuditEvent, AuditEventFilter, Candidate, Conversation, FollowUpPolicy, FollowUpTask,
    FollowUpTaskStatus, GeneratedDraft, Message, Position, SendRecord,
};
use serde_json::Value;
use thiserror::Error;

use crate::error::ApplicationError;
use crate::payload::{
    CandidateFilter, CandidatePatch, ConversationFilter, DraftFilter, FollowUpPolicyPatch,
    FollowUpResultPatch, FollowUpTaskFilter, FollowUpTaskPatch, MessageFilter, NewCandidate,
    NewConversation, NewMessage, NewPosition, NewSendRecord, PositionPatch, SendRecordFilter,
};

/// Filter the repository applies when listing positions.
///
/// ``search`` is already normalised by the domain
/// (``ica_domain::normalize_search``): ``None`` means "no text filter".
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PositionFilter {
    pub include_archived: bool,
    pub search: Option<String>,
    /// SEC-04-B: when present, the SQLite adapter MUST restrict rows to this
    /// tenant and (for a non-admin actor) to rows they own. `None` preserves
    /// the documented single-user Local mode.
    pub scope: Option<crate::Scope>,
}

/// A persistence failure.
///
/// Most failures are "the database could not be read or written": the message
/// is logged and surfaced as a generic 500, never echoed (it can carry SQL
/// details).
///
/// A few are **business-rule failures** the baseline reports as 409
/// `business_rule_violation` — the follow-up state machine refusing an
/// unreachable transition, for example. Those carry a message and a details
/// map that are *contract text*, so they are modelled separately: turning one
/// into a 500 would tell the client "the server crashed" for what is
/// actually "you asked for something the rules forbid".
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("{message}")]
pub struct RepositoryError {
    message: String,
    /// The classification is also the error source: it carries why the
    /// operation failed (§10.1 `#[source]`), without changing Display text.
    #[source]
    kind: RepositoryErrorKind,
}

/// Which envelope this failure must become.
///
/// The three non-500 cases are genuinely different responses in the baseline
/// and must not be collapsed: `NotFoundError` is a 404, `BusinessRuleError` a
/// 409, `ValidationError` a 422. Storing them as separate data (instead of "a
/// details map means 409") is what lets a 404 and a 422 both carry details and
/// still be told apart.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RepositoryErrorKind {
    /// A persistence failure: 500, message not echoed.
    #[error("persistence failure")]
    Unavailable,
    /// The row does not exist: 404.
    #[error("entity not found")]
    NotFound,
    /// A business-rule refusal: 409 with `details`.
    #[error("business rule violation")]
    Conflict(BTreeMap<String, String>),
    /// A validation refusal: 422 with `details` and a caller-chosen code.
    ///
    /// The code matters: an import payload that cannot be read back is
    /// `import_validation_error`, not the generic `validation_error`.
    /// `details` is free-form JSON: the import validator's `details.errors`
    /// is an ARRAY of row problems on the wire (the baseline's
    /// `details={"errors": problems}`), not a stringified list — a client
    /// parses it, it does not `json.loads` it.
    #[error("validation failure: {code}")]
    Invalid { code: &'static str, details: Value },
}

impl RepositoryError {
    pub fn unavailable(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            kind: RepositoryErrorKind::Unavailable,
        }
    }

    /// A business-rule failure: the baseline's `BusinessRuleError` (409).
    pub fn conflict(message: impl Into<String>, details: BTreeMap<String, String>) -> Self {
        Self {
            message: message.into(),
            kind: RepositoryErrorKind::Conflict(details),
        }
    }

    /// The row does not exist: the baseline's `NotFoundError` (404).
    pub fn not_found(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            kind: RepositoryErrorKind::NotFound,
        }
    }

    /// A validation refusal: the baseline's `ValidationError` (422).
    pub fn invalid(message: impl Into<String>, details: Value) -> Self {
        Self {
            message: message.into(),
            kind: RepositoryErrorKind::Invalid {
                code: "validation_error",
                details,
            },
        }
    }

    /// A validation refusal carrying a specific code (`import_validation_error`).
    pub fn invalid_with_code(
        code: &'static str,
        message: impl Into<String>,
        details: Value,
    ) -> Self {
        Self {
            message: message.into(),
            kind: RepositoryErrorKind::Invalid { code, details },
        }
    }

    /// Whether this is a "no such row" failure (404).
    pub fn is_not_found(&self) -> bool {
        matches!(self.kind, RepositoryErrorKind::NotFound)
    }

    pub fn kind(&self) -> &RepositoryErrorKind {
        &self.kind
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    /// Wire `code` of the [`ApplicationError`] this failure maps to.
    /// Kept allocation-free so the `#[from]` bridge can use `&'static str`.
    pub fn application_code(&self) -> &'static str {
        match &self.kind {
            RepositoryErrorKind::NotFound => "not_found",
            RepositoryErrorKind::Conflict(_) => "business_rule_violation",
            RepositoryErrorKind::Invalid { code, .. } => code,
            RepositoryErrorKind::Unavailable => "internal_error",
        }
    }

    /// HTTP status of the [`ApplicationError`] this failure maps to.
    pub fn application_status(&self) -> u16 {
        match &self.kind {
            RepositoryErrorKind::NotFound => 404,
            RepositoryErrorKind::Conflict(_) => 409,
            RepositoryErrorKind::Invalid { .. } => 422,
            RepositoryErrorKind::Unavailable => 500,
        }
    }

    /// The business-rule details, when this is a 409 rather than a 500.
    pub fn conflict_details(&self) -> Option<&BTreeMap<String, String>> {
        match &self.kind {
            RepositoryErrorKind::Conflict(details) => Some(details),
            _ => None,
        }
    }

    /// Whether this is a business-rule failure (409) rather than a
    /// persistence failure (500).
    pub fn is_conflict(&self) -> bool {
        matches!(self.kind, RepositoryErrorKind::Conflict(_))
    }
}

impl RepositoryError {
    /// Map a persistence failure onto the application-layer error.
    ///
    /// The three shapes must stay distinct, because the baseline's envelopes
    /// are distinct: a missing row is a **404**, a business-rule refusal is a
    /// **409**, and anything else is a **500**. Collapsing "not found" into
    /// either of the others would ship the wrong status code.
    pub fn application_error(&self) -> crate::error::ApplicationError {
        match &self.kind {
            RepositoryErrorKind::NotFound => crate::error::ApplicationError::NotFound {
                message: self.message.clone(),
            },
            RepositoryErrorKind::Conflict(details) => crate::error::ApplicationError::Conflict {
                message: self.message.clone(),
                details: details.clone(),
            },
            RepositoryErrorKind::Invalid { code, details } => {
                crate::error::ApplicationError::ValidationWithCode {
                    code,
                    message: self.message.clone(),
                    details: details.clone(),
                }
            }
            RepositoryErrorKind::Unavailable => crate::error::ApplicationError::Unavailable {
                message: self.message.clone(),
            },
        }
    }
}

/// Read and write access to positions.
#[async_trait]
pub trait PositionRepository: Send + Sync {
    /// Every position matching ``filter``, newest ``created_at`` first.
    async fn list(&self, filter: PositionFilter) -> Result<Vec<Position>, RepositoryError>;

    /// One page of positions plus the real filtered count (OPT-2).
    ///
    /// The default implementation slices in memory, so a test double stays a
    /// two-method fake; the SQLite adapter overrides it with `COUNT(*)` +
    /// `LIMIT/OFFSET`. Either way `total` is the count *before* the window and
    /// the items are the same rows in the same order.
    async fn page(
        &self,
        filter: PositionFilter,
        offset: usize,
        limit: usize,
    ) -> Result<(Vec<Position>, usize), RepositoryError> {
        let rows = self.list(filter).await?;
        let total = rows.len();
        let items = rows.into_iter().skip(offset).take(limit).collect();
        Ok((items, total))
    }

    /// One position by id, or ``None`` when it does not exist.
    async fn find(&self, id: &str) -> Result<Option<Position>, RepositoryError>;

    /// SEC-04-B/C: same lookup, but a real identity may only see rows in its
    /// own tenant (and, for members, rows it owns). The default keeps legacy
    /// Local-mode test doubles working; the SQLite adapter overrides it.
    async fn find_scoped(
        &self,
        id: &str,
        _scope: &crate::Scope,
    ) -> Result<Option<Position>, RepositoryError> {
        self.find(id).await
    }

    /// Insert one position and its audit event in a single transaction.
    async fn create(&self, new: NewPosition) -> Result<Position, RepositoryError>;

    /// SEC-04-B/C: insert with the authenticated actor's ownership. The
    /// default keeps legacy Local-mode doubles unchanged.
    async fn create_scoped(
        &self,
        new: NewPosition,
        _scope: &crate::Scope,
    ) -> Result<Position, RepositoryError> {
        self.create(new).await
    }

    /// Apply a patch; ``Ok(None)`` means the row does not exist.
    async fn update(
        &self,
        id: &str,
        patch: PositionPatch,
    ) -> Result<Option<Position>, RepositoryError>;

    /// SEC-04-B/C: patch only a row inside the caller's scope. The default
    /// keeps legacy Local-mode doubles unchanged.
    async fn update_scoped(
        &self,
        id: &str,
        patch: PositionPatch,
        _scope: &crate::Scope,
    ) -> Result<Option<Position>, RepositoryError> {
        self.update(id, patch).await
    }

    /// Delete one position; ``Ok(false)`` means the row does not exist.
    ///
    /// The FK is `ON DELETE SET NULL`, so linked candidates keep their row and
    /// lose only the link.
    async fn delete(&self, id: &str) -> Result<bool, RepositoryError>;

    /// SEC-04-B/C: delete only a row inside the caller's scope.
    async fn delete_scoped(
        &self,
        id: &str,
        _scope: &crate::Scope,
    ) -> Result<bool, RepositoryError> {
        self.delete(id).await
    }
}

/// Read and write access to candidates.
#[async_trait]
pub trait CandidateRepository: Send + Sync {
    /// One page of candidates plus the real filtered count.
    async fn list(
        &self,
        filter: CandidateFilter,
        offset: usize,
        limit: usize,
    ) -> Result<(Vec<Candidate>, usize), RepositoryError>;

    /// One candidate with its derived counters, or ``None``.
    async fn find(&self, id: &str) -> Result<Option<Candidate>, RepositoryError>;

    /// The full position row a candidate is linked to, with `candidate_count`.
    ///
    /// Used by the detail response: mapping the position independently is what
    /// keeps its nested `candidate_count` real instead of the default 0.
    async fn position_of(&self, candidate_id: &str) -> Result<Option<Position>, RepositoryError>;

    async fn create(&self, new: NewCandidate) -> Result<Candidate, RepositoryError>;

    async fn update(
        &self,
        id: &str,
        patch: CandidatePatch,
    ) -> Result<Option<Candidate>, RepositoryError>;

    async fn delete(&self, id: &str) -> Result<bool, RepositoryError>;
}

/// Read and write access to conversations.
#[async_trait]
pub trait ConversationRepository: Send + Sync {
    async fn list(
        &self,
        filter: ConversationFilter,
        offset: usize,
        limit: usize,
    ) -> Result<(Vec<Conversation>, usize), RepositoryError>;

    async fn find(&self, id: &str) -> Result<Option<Conversation>, RepositoryError>;

    /// The candidate's most recent active conversation
    /// (`last_message_at DESC NULLS LAST`, first row), if any.
    async fn find_active_for_candidate(
        &self,
        candidate_id: &str,
    ) -> Result<Option<Conversation>, RepositoryError>;

    async fn create(&self, new: NewConversation) -> Result<Conversation, RepositoryError>;

    /// Create a conversation row **without** an audit event.
    ///
    /// `POST /api/messages` opens a conversation implicitly when the candidate
    /// has none; the baseline audits only the explicit
    /// `POST /api/conversations`, so the implicit path must not leave a
    /// `conversation.created` trail behind.
    async fn create_implicit(
        &self,
        candidate_id: &str,
        channel: &str,
    ) -> Result<Conversation, RepositoryError>;
}

/// Read and write access to messages.
#[async_trait]
pub trait MessageRepository: Send + Sync {
    /// One page of messages plus the real filtered count (IMP-1/IMP-3).
    async fn list(
        &self,
        filter: MessageFilter,
        offset: usize,
        limit: usize,
    ) -> Result<(Vec<Message>, usize), RepositoryError>;

    /// Insert a message, bump its conversation's `last_message_at` and write the
    /// `message.created` audit event, all in one transaction.
    async fn create(
        &self,
        new: NewMessage,
        conversation_id: &str,
    ) -> Result<Message, RepositoryError>;

    /// The candidate's messages oldest-first (`message_time ASC NULLS LAST`,
    /// then `created_at ASC`), capped at `limit` — the AI context builder's
    /// read (W3).
    async fn context_messages(
        &self,
        candidate_id: &str,
        limit: i64,
    ) -> Result<Vec<Message>, RepositoryError>;
}

/// Read and write access to manual send records.
#[async_trait]
pub trait SendRecordRepository: Send + Sync {
    async fn list(
        &self,
        filter: SendRecordFilter,
        offset: usize,
        limit: usize,
    ) -> Result<(Vec<SendRecord>, usize), RepositoryError>;

    /// Append one record and its `send.recorded` audit event atomically.
    async fn create(&self, new: NewSendRecord) -> Result<SendRecord, RepositoryError>;

    /// Candidate a follow-up task belongs to, or `None` when it does not exist.
    ///
    /// A narrow read of the W2 table: `POST /api/send-records` has to reject a
    /// task that belongs to another candidate, and that check needs one column.
    async fn follow_up_candidate(&self, task_id: &str) -> Result<Option<String>, RepositoryError>;

    /// `(candidate_id, status)` of a draft, or `None` when it does not exist.
    async fn draft_candidate_status(
        &self,
        draft_id: &str,
    ) -> Result<Option<(String, String)>, RepositoryError>;
}

/// One row of `follow_up_policies` as the application sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FollowUpPolicyRecord {
    pub id: String,
    pub name: String,
    pub policy: FollowUpPolicy,
    pub is_active: bool,
}

/// Read and write access to follow-up tasks and their policy.
///
/// Every write method also writes the audit event the baseline writes for the
/// same operation, inside the same transaction — an audit event can never
/// describe a change that rolled back.
#[async_trait]
pub trait FollowUpRepository: Send + Sync {
    /// One page of tasks plus the real filtered count (`created_at DESC`).
    async fn list(
        &self,
        filter: FollowUpTaskFilter,
        offset: usize,
        limit: usize,
    ) -> Result<(Vec<FollowUpTask>, usize), RepositoryError>;

    /// One task by id, or `None`.
    async fn find(&self, id: &str) -> Result<Option<FollowUpTask>, RepositoryError>;

    /// The candidate's open task — not terminal, not blocked — newest first.
    /// Task creation returns this row instead of a duplicate.
    async fn find_open_for_candidate(
        &self,
        candidate_id: &str,
    ) -> Result<Option<FollowUpTask>, RepositoryError>;

    /// Insert one task and its `follow_up.task_created` audit atomically.
    async fn create(&self, new: NewFollowUpTaskRow) -> Result<FollowUpTask, RepositoryError>;

    /// Write the `follow_up.task_reused` audit for an existing open task and
    /// return the row.
    async fn mark_reused(
        &self,
        id: &str,
        status: FollowUpTaskStatus,
    ) -> Result<Option<FollowUpTask>, RepositoryError>;

    /// Apply a PATCH (the status machine has already been validated by the
    /// application) and write `follow_up.task_updated`.
    async fn update(
        &self,
        id: &str,
        patch: FollowUpTaskPatch,
    ) -> Result<Option<FollowUpTask>, RepositoryError>;

    /// Cancel with the machine check already applied; write
    /// `follow_up.task_cancelled`. `Some(reason)` replaces the task reason.
    async fn cancel(
        &self,
        id: &str,
        reason: Option<String>,
    ) -> Result<Option<FollowUpTask>, RepositoryError>;

    /// Complete; write `follow_up.task_completed`.
    async fn complete(
        &self,
        id: &str,
        completed_at: DateTime<Utc>,
    ) -> Result<Option<FollowUpTask>, RepositoryError>;

    /// Record a result, write `follow_up.result_recorded`, and — when the
    /// caller built one — append the send record with its `send.recorded`
    /// audit. One transaction, exactly like the baseline's `record_result`.
    async fn record_result(
        &self,
        id: &str,
        patch: FollowUpResultPatch,
        send_record: Option<NewSendRecord>,
    ) -> Result<Option<FollowUpTask>, RepositoryError>;

    /// Expire every task past `expires_at` whose state legally reaches
    /// `Expired`, stamp `updated_at`, and write the single
    /// `follow_up.tasks_expired` audit (no entity). Returns the count.
    async fn expire_stale(&self, now: DateTime<Utc>) -> Result<u64, RepositoryError>;

    /// Move a task to `target` along the shortest legal path (the baseline's
    /// `advance_task`): stamps every intermediate status, optionally sets the
    /// reason, and writes ONE `follow_up.task_advanced` audit with the path as
    /// metadata. A task already in `target` is left untouched.
    async fn advance(
        &self,
        id: &str,
        target: FollowUpTaskStatus,
        reason: Option<String>,
    ) -> Result<Option<FollowUpTask>, RepositoryError>;

    /// The candidate's latest message
    /// (`message_time DESC NULLS LAST`, then `created_at DESC`).
    async fn latest_message_for_candidate(
        &self,
        candidate_id: &str,
    ) -> Result<Option<Message>, RepositoryError>;

    /// How many of the candidate's tasks count as real contact attempts.
    async fn contact_attempt_count(&self, candidate_id: &str) -> Result<i64, RepositoryError>;

    /// The most recent contact-counting task
    /// (`last_follow_up_at DESC NULLS LAST`) — the interval reference point.
    async fn latest_contact_task(
        &self,
        candidate_id: &str,
    ) -> Result<Option<FollowUpTask>, RepositoryError>;

    /// The task's only sendable (approved/copied) draft id, or `None` when
    /// there is none or several — the baseline refuses to guess provenance.
    async fn sole_sendable_draft_id(
        &self,
        task_id: &str,
    ) -> Result<Option<String>, RepositoryError>;

    /// The active policy row (`is_active = true`, oldest first), or `None`.
    async fn active_policy(&self) -> Result<Option<FollowUpPolicyRecord>, RepositoryError>;

    /// The policy row by its unique name, or `None`.
    async fn policy_by_name(
        &self,
        name: &str,
    ) -> Result<Option<FollowUpPolicyRecord>, RepositoryError>;

    /// Insert the default policy row (`follow_up_policies`, no audit).
    async fn create_default_policy(&self) -> Result<FollowUpPolicyRecord, RepositoryError>;

    /// Patch the policy row and write `follow_up.policy_updated` with the
    /// applied fields as metadata (NULL for an empty patch).
    async fn update_policy(
        &self,
        id: &str,
        patch: &FollowUpPolicyPatch,
    ) -> Result<FollowUpPolicyRecord, RepositoryError>;
}

/// A task row the application has fully decided: status derived from the
/// policy evaluation, expiry stamped, defaults filled. `outcome` is the
/// decision outcome string the `follow_up.task_created` audit metadata carries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewFollowUpTaskRow {
    pub candidate_id: String,
    pub position_id: Option<String>,
    pub status: FollowUpTaskStatus,
    pub outcome: String,
    pub objective: Option<String>,
    pub reason: Option<String>,
    pub block_reason: Option<String>,
    pub recommended_at: Option<DateTime<Utc>>,
    pub expires_at: DateTime<Utc>,
}

/// Aggregated counts for `GET /api/dashboard`, plus the maintenance sweeps the
/// baseline runs on the same read path.
#[async_trait]
pub trait DashboardRepository: Send + Sync {
    /// Message counts per UTC day for the workbench calendar
    /// (`dashboard.py` 的 `messages_by_day`): days are
    /// `date(coalesce(message_time, created_at))`, only on/after `month_start`.
    async fn messages_by_day(
        &self,
        month_start: chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<(String, i64)>, RepositoryError>;

    /// Hard-delete candidates `archived` and stale past `cutoff` (FK cascade
    /// removes their children). Returns the deleted count.
    async fn expire_old_candidates(&self, cutoff: DateTime<Utc>) -> Result<u64, RepositoryError>;

    /// Delete audit events older than `cutoff`. Returns the deleted count.
    async fn prune_audit_events(&self, cutoff: DateTime<Utc>) -> Result<u64, RepositoryError>;

    /// Blank `raw_text` on import sessions older than `cutoff`. Returns the
    /// cleared count.
    async fn prune_stale_import_raw_text(
        &self,
        cutoff: DateTime<Utc>,
    ) -> Result<u64, RepositoryError>;

    /// Candidates with `archived = false`.
    async fn candidate_total(&self) -> Result<i64, RepositoryError>;

    /// `(status, count)` for candidates with `archived = false`.
    async fn candidate_status_counts(&self) -> Result<Vec<(String, i64)>, RepositoryError>;

    /// Tasks in `statuses`, `recommended_at ASC NULLS LAST`, at most `limit`.
    async fn pending_tasks(
        &self,
        statuses: &[FollowUpTaskStatus],
        limit: usize,
    ) -> Result<Vec<FollowUpTask>, RepositoryError>;

    /// Tasks in `statuses`, counted.
    async fn task_count_in(&self, statuses: &[FollowUpTaskStatus]) -> Result<i64, RepositoryError>;

    /// Drafts whose status is one of `statuses` (lower-case strings), counted.
    async fn draft_count_in(&self, statuses: &[&str]) -> Result<i64, RepositoryError>;

    /// The `limit` newest messages (`created_at DESC`).
    async fn recent_messages(&self, limit: usize) -> Result<Vec<Message>, RepositoryError>;

    /// Positions with `archived = false`.
    async fn position_total(&self) -> Result<i64, RepositoryError>;

    /// Positions with `archived = false` and `status = 'Open'`.
    async fn open_position_count(&self) -> Result<i64, RepositoryError>;
}

// ── Drafts and the AI port (W3) ────────────────────────────────────────────

/// Read and write access to generated drafts. Every transition method writes
/// the audit event the baseline writes for the same action, inside the same
/// transaction; the follow-up-task advance happens through
/// [`FollowUpRepository::advance`], which the caller drives.
#[async_trait]
pub trait DraftRepository: Send + Sync {
    /// One page of drafts plus the real filtered count (`created_at DESC`).
    async fn list(
        &self,
        filter: DraftFilter,
        offset: usize,
        limit: usize,
    ) -> Result<(Vec<GeneratedDraft>, usize), RepositoryError>;

    /// One draft by id, or `None`.
    async fn find(&self, id: &str) -> Result<Option<GeneratedDraft>, RepositoryError>;

    /// Every draft of one comparison group (unordered; the use case sorts).
    async fn find_group(&self, group_id: &str) -> Result<Vec<GeneratedDraft>, RepositoryError>;

    /// The egress-consent switch (`app_settings.key = 'ai.egress_consent'`);
    /// defaults to allowed when nothing is recorded (PRIV-07).
    async fn egress_consent(&self) -> Result<bool, RepositoryError>;

    /// Apply a wording/objective edit, set `is_edited`/status when the text
    /// changed, stamp `updated_at`, and write `draft.edited`.
    async fn update(
        &self,
        id: &str,
        message_text: Option<String>,
        objective: Option<String>,
    ) -> Result<Option<GeneratedDraft>, RepositoryError>;

    /// Mark approved (`approved_at` stamped) and write `draft.approved` with
    /// the request note as metadata (kept even when null, like the baseline).
    async fn approve(
        &self,
        id: &str,
        approved_at: DateTime<Utc>,
        note: Option<String>,
    ) -> Result<Option<GeneratedDraft>, RepositoryError>;

    /// Mark rejected (`rejected_at` stamped) and write `draft.rejected`.
    async fn reject(
        &self,
        id: &str,
        rejected_at: DateTime<Utc>,
    ) -> Result<Option<GeneratedDraft>, RepositoryError>;

    /// Mark copied (`copied_at` stamped) and write `draft.copied`.
    async fn copy(
        &self,
        id: &str,
        copied_at: DateTime<Utc>,
    ) -> Result<Option<GeneratedDraft>, RepositoryError>;

    /// Mark every **other** group member rejected with `rejected_at` and one
    /// `draft.rejected` audit each (metadata `superseded_by`/`reason`).
    /// Returns the updated rows in the caller's order.
    async fn supersede_others(
        &self,
        group_id: &str,
        selected_id: &str,
        rejected_at: DateTime<Utc>,
    ) -> Result<Vec<GeneratedDraft>, RepositoryError>;
}

// ── Import sessions, OCR and data snapshots (W4) ───────────────────────────

/// The parsed records a preview should persist, plus the session envelope.
///
/// `preview` is not a read-only call: the baseline writes the session, its
/// records and an `import.previewed` audit row. What it must never touch is the
/// business tables (`candidates`, `messages`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewImportSessionRow {
    pub candidate_id: Option<String>,
    /// `MessageSource` as stored (`paste_import` / `screenshot_ocr`).
    pub source: String,
    pub raw_text: String,
    pub status: ica_domain::ImportStatus,
    pub error_message: Option<String>,
    pub records: Vec<NewImportRecordRow>,
    pub detected_duplicate_count: i64,
}

/// One record to persist with a preview.
///
/// Neither `id` nor `session_id` is carried: the session does not exist yet
/// when the parser produces these, so the repository mints both and returns the
/// finished rows inside [`ica_domain::ImportSession`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewImportRecordRow {
    pub sequence: i64,
    pub sender_type: Option<String>,
    pub content: String,
    pub message_time: Option<DateTime<Utc>>,
    pub status: ica_domain::RecordStatus,
    pub duplicate_of_message_id: Option<String>,
    pub content_hash: Option<String>,
    pub validation_messages: Option<String>,
}

/// What a confirmation produced.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ImportConfirmOutcome {
    pub session_id: String,
    pub saved_message_ids: Vec<String>,
    pub skipped_count: i64,
    pub duplicate_count: i64,
}

/// One per-record correction, already validated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportRecordCorrection {
    pub id: String,
    pub sender_type: Option<ica_domain::SenderType>,
    pub content: Option<String>,
    pub message_time: Option<DateTime<Utc>>,
    pub skip: bool,
}

/// Read and write access to import sessions.
///
/// Every write method also writes the audit event the baseline writes for the
/// same action, inside the same transaction.
#[async_trait]
pub trait ImportRepository: Send + Sync {
    /// Whether a candidate exists — `preview` validates it before writing.
    async fn candidate_exists(&self, id: &str) -> Result<bool, RepositoryError>;

    /// `content_hash -> message id` for the candidate's existing messages, the
    /// input to duplicate detection.
    async fn message_hashes(
        &self,
        candidate_id: &str,
    ) -> Result<std::collections::HashMap<String, String>, RepositoryError>;

    /// Persist a preview: session + records + `import.previewed`, one
    /// transaction. `preview` must not write `messages`.
    async fn create_preview(
        &self,
        new: NewImportSessionRow,
    ) -> Result<ica_domain::ImportSession, RepositoryError>;

    /// One session with its records, or `None`.
    async fn find(&self, id: &str) -> Result<Option<ica_domain::ImportSession>, RepositoryError>;

    /// Confirm a session: write one `messages` row per record that is neither
    /// skipped nor an unflagged duplicate, stamp the session confirmed, and
    /// write `import.confirmed` — all in one transaction, so a failure leaves
    /// no partial messages.
    async fn confirm(
        &self,
        session_id: &str,
        candidate_id: &str,
        conversation_id: Option<String>,
        corrections: Vec<ImportRecordCorrection>,
    ) -> Result<ImportConfirmOutcome, RepositoryError>;

    /// Reject a session: set `rejected`, clear `raw_text`, write
    /// `import.rejected`.
    ///
    /// Note the baseline still lets a rejected session be confirmed afterwards
    /// (its confirm guards `confirmed` only); this port copies that.
    async fn reject(&self, id: &str) -> Result<Option<ica_domain::ImportSession>, RepositoryError>;
}

/// Local OCR, used only by `POST /imports/ocr`.
///
/// `ica-application` never touches an image or an OCR engine; the adapter
/// decides how text is read. A missing engine must surface as a readable error
/// (the baseline's `require_ocr`), never a panic or a 500.
#[async_trait]
pub trait OcrEngine: Send + Sync {
    /// Fail with the baseline's "OCR is not available" error when the engine is
    /// missing.
    fn ensure_available(&self) -> Result<(), ApplicationError>;

    /// Read the image at `path` and return the recognised text.
    async fn read_text(&self, path: &std::path::Path) -> Result<String, ApplicationError>;
}

/// The JSON snapshot an export produces, and the outcome an import reports.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SnapshotOutcome {
    pub imported: i64,
    pub updated: i64,
    pub skipped: i64,
    pub conflicts: i64,
}

/// Read and write access to the whole data snapshot (`POST /export`,
/// `POST /import`).
#[async_trait]
pub trait DataSnapshotRepository: Send + Sync {
    /// Every exported table, as `table name -> rows`.
    async fn export_tables(
        &self,
    ) -> Result<serde_json::Map<String, serde_json::Value>, RepositoryError>;

    /// Import a snapshot: known UUIDs merged last-write-wins on `updated_at`,
    /// unknown UUIDs inserted, cross-tenant/owner violations rejecting the
    /// whole request before anything is written. One transaction.
    async fn import_tables(
        &self,
        data: serde_json::Map<String, serde_json::Value>,
    ) -> Result<SnapshotOutcome, RepositoryError>;

    /// Write the `data.exported` audit row.
    ///
    /// Called only after the document exists — the baseline deliberately orders
    /// the audit after key validation and sealing, so a refused export leaves
    /// no success event. Only table names and row counts are recorded, never
    /// the exported content.
    async fn record_export_audit(
        &self,
        outcome: &crate::sync::ExportOutcome,
    ) -> Result<(), RepositoryError>;

    /// Write the `data.imported` audit row, with the counts it reports.
    async fn record_import_audit(
        &self,
        outcome: &SnapshotOutcome,
        tables: &[String],
    ) -> Result<(), RepositoryError>;
}

/// Seals an export snapshot with AES-256-GCM (`X-Encrypt-Aes-Key`).
///
/// The key is base64 of exactly 32 bytes; an unusable key must be refused
/// rather than ignored, so an encrypted export can never silently fall back to
/// plaintext.
pub trait ExportSealer: Send + Sync {
    fn seal(
        &self,
        document: &serde_json::Value,
        key_b64: &str,
    ) -> Result<(String, String, String), ApplicationError>;
}

/// What one generation produced: a single draft, or the whole A/B comparison
/// group (the baseline's `generate_draft` returning `GeneratedDraft |
/// list[DraftRead]`).
pub enum DraftGeneration {
    Single(Box<GeneratedDraft>),
    Group(Vec<GeneratedDraft>),
}

/// The AI generation port. `ica-application` never talks to a provider: the
/// adapter implements this against whatever backend is configured — model
/// registry, routing, budget gate, key resolution, the HTTP call, validation
/// and persistence all live behind it (the baseline's
/// `app/ai/service.py::generate_draft`).
#[async_trait]
pub trait DraftGenerator: Send + Sync {
    async fn generate(
        &self,
        request: crate::ai_context::GenerateDraftRequest,
    ) -> Result<DraftGeneration, ApplicationError>;
}

// ── Browser candidate extraction (W7) ──────────────────────────────────────

/// The request to confirm a reviewed browser extraction draft
/// (`CandidateConfirmRequest`). `confirm_duplicate` is the only way past a
/// duplicate verdict, and it is an explicit human statement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateConfirm {
    pub position_id: Option<String>,
    pub status: ica_domain::CandidateStatus,
    pub confirm_duplicate: bool,
    pub note: Option<String>,
}

/// The result of confirming (`CandidateConfirmResult`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateConfirmOutcome {
    pub session_id: String,
    pub extraction_id: String,
    pub candidate_id: String,
    pub duplicate_kind: Option<String>,
    pub duplicate_override: bool,
    pub fields_written: Vec<String>,
}

/// One browser candidate-extraction draft, as the confirm path reads it
/// (the staging row of `browser_candidate_extractions`; NOT a candidate).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractionDraft {
    pub id: String,
    pub session_id: String,
    pub snapshot_action_id: String,
    pub import_session_id: Option<String>,
    pub source_url: String,
    pub source_domain: String,
    pub snapshot_sha256: String,
    pub fields_json: String,
    pub duplicate_kind: Option<String>,
    pub duplicate_of_candidate_id: Option<String>,
    pub duplicate_evidence_json: Option<String>,
    pub status: String,
    pub review_digest: Option<String>,
    /// Set when the draft was confirmed: the candidate it became.
    pub candidate_id: Option<String>,
}

impl ExtractionDraft {
    /// `_load_fields` + `_present_fields`: the six known field slots read from
    /// the stored JSON, keeping only non-empty values (a masked-out field is
    /// null or "" and contributes nothing — not to the digest, not to
    /// `fields_written`).
    pub fn present_fields(&self) -> Vec<(&'static str, String)> {
        let parsed: serde_json::Value =
            serde_json::from_str(&self.fields_json).unwrap_or(serde_json::Value::Null);
        ica_domain::candidate_extraction::FIELD_NAMES
            .iter()
            .filter_map(|name| {
                let value = parsed.get(*name)?;
                let text = value.as_str()?;
                if text.is_empty() {
                    return None;
                }
                Some((*name, text.to_string()))
            })
            .collect()
    }

    /// The digest of the fields AS THEY SIT NOW (`domain.review_digest` over
    /// the present fields).
    pub fn review_digest_now(&self) -> String {
        let fields = self.present_fields();
        ica_domain::candidate_extraction::review_digest(
            &fields
                .iter()
                .map(|(name, value)| (*name, Some(value.as_str())))
                .collect::<Vec<_>>(),
        )
    }
}
#[async_trait]
pub trait BrowserExtractionRepository: Send + Sync {
    /// The draft bound to one import session (`extraction_for_session`): the
    /// first row whose `import_session_id` matches. `None` when the session
    /// has no browser draft behind it.
    async fn find_by_import_session(
        &self,
        session_id: &str,
    ) -> Result<Option<ExtractionDraft>, RepositoryError>;

    /// Audit + commit a refused duplicate confirmation (`EVENT_DUPLICATE_BLOCKED`).
    /// The baseline COMMITS the block audit before raising, so the refusal is
    /// queryable after the fact.
    async fn record_duplicate_block(
        &self,
        extraction: &ExtractionDraft,
        session_id: &str,
    ) -> Result<(), RepositoryError>;

    /// Write an approved draft into `candidates` in one transaction: the
    /// candidate INSERT + its audit, the extraction row's confirmation, the
    /// session's confirmation and the
    /// `browser.candidate_extraction.confirmed` audit all commit together.
    /// The GUARDS live in the use case; this only executes the write path.
    async fn confirm_candidate(
        &self,
        session_id: &str,
        draft: &ExtractionDraft,
        request: CandidateConfirm,
    ) -> Result<CandidateConfirmOutcome, RepositoryError>;
}

/// The audit trail, read back (W5 `GET /api/audit-events`).
///
/// Writes go through each aggregate's own transaction via `ica-infra::audit`;
/// this port is the read side only. `filter.limit` is already validated by
/// the transport layer (`1..=500`); the port applies no clamping of its own.
#[async_trait]
pub trait AuditTrailRepository: Send + Sync {
    /// The newest events first (`ORDER BY created_at DESC`), exactly the
    /// baseline's `audit_recent`.
    async fn recent_events(
        &self,
        filter: AuditEventFilter,
    ) -> Result<Vec<AuditEvent>, RepositoryError>;
}

/// The non-sensitive key/value `app_settings` store (W5 settings slice).
///
/// The AI-egress switch lives here so it survives a restart and is visible to
/// every process; its changes are audited in the same transaction.
#[async_trait]
pub trait AppSettingsRepository: Send + Sync {
    /// The `ai.egress_consent` switch: **`true` when nothing is recorded**
    /// (configuring a provider key is itself an explicit operator action),
    /// and the truthy set is the baseline's `{"1","true","yes","on"}`.
    async fn egress_consent_granted(&self) -> Result<bool, RepositoryError>;

    /// Upsert the switch and append its
    /// `ai.egress_consent.granted`/`.revoked` audit event **in one
    /// transaction**, exactly like every other business write.
    async fn set_egress_consent(&self, granted: bool) -> Result<(), RepositoryError>;

    /// Upsert non-sensitive values WITHOUT an audit event — the budget pair
    /// (`PATCH /ai/budget`) writes rows the baseline never reads back and
    /// never audits; reproduced, not improved.
    async fn upsert_values(&self, entries: &[(&str, String, &str)]) -> Result<(), RepositoryError>;

    /// Append an `ai.key_configured`/`ai.key_cleared` row in its own
    /// transaction — the credential write is external to the database, so
    /// the audit is the only DB side effect of the ai-key endpoints.
    async fn record_ai_key_audit(
        &self,
        event_type: &str,
        summary: String,
        metadata: serde_json::Value,
    ) -> Result<(), RepositoryError>;
}

/// The `ai_model_configs` registry read side and the `ai_model_routing`
/// overrides (W5). The mutating model-CRUD routes are AI-EXT and stay out.
#[async_trait]
pub trait AiModelRepository: Send + Sync {
    /// Every row, ordered by `sort_order` (the registry's load order).
    async fn list_configs(&self) -> Result<Vec<AiModelData>, RepositoryError>;

    /// The stored overrides, as `(message_type, tier)` pairs.
    async fn routing_overrides(&self) -> Result<Vec<(String, String)>, RepositoryError>;

    /// Insert/update/delete one override: `None` deletes the row so the
    /// default shows through again.
    async fn replace_routing_override(
        &self,
        message_type: &str,
        tier: Option<&str>,
    ) -> Result<(), RepositoryError>;
}

/// The `ai_usage_daily` read side (W5 usage/budget).
#[async_trait]
pub trait AiUsageRepository: Send + Sync {
    /// Every row, ordered by `usage_date` (the usage endpoint's scan order).
    async fn list_rows(&self) -> Result<Vec<AiUsageRow>, RepositoryError>;

    /// Record one call's token usage (`BudgetGuard.record_usage`): the
    /// `(usage_date, model_id)` row is created or incremented — tokens, cost
    /// and `request_count` all accumulate.
    async fn record_usage(
        &self,
        model_id: &str,
        prompt_tokens: i64,
        completion_tokens: i64,
        cost_cents: i64,
    ) -> Result<(), RepositoryError>;
}

/// The OS credential store seam (W5 ai-key).
///
/// The baseline stores AI credentials per slot (installation-wide
/// `ai_api_key`, per-model `ai_key:<model_id>`) through the platform store —
/// DPAPI on Windows, Keychain on macOS. The project ruling for this port is
/// the OS credential store (the `keyring` adapter); tests use an in-memory
/// fake so no run ever writes the operator's real credential vault.
#[async_trait]
pub trait SecretStore: Send + Sync {
    /// Persist one slot. Failures surface as 500s, never as silent loss.
    async fn store(&self, name: &str, value: &str) -> Result<(), RepositoryError>;
    /// Read one slot, or `None` when absent/unreadable (a broken store must
    /// not block resolution — the baseline's `load_secret` degrades too).
    async fn load(&self, name: &str) -> Result<Option<String>, RepositoryError>;
    /// Remove one slot.
    async fn delete(&self, name: &str) -> Result<(), RepositoryError>;
}

/// One backup file on disk (`BackupRead`).
#[derive(Debug, Clone, PartialEq)]
pub struct BackupRecord {
    pub name: String,
    pub path: String,
    pub size_bytes: i64,
    /// ISO-8601 with UTC offset, from the file mtime.
    pub created_at: String,
    /// `"backup"` or `"pre_restore"` — both families are listed.
    pub kind: String,
}

/// The outcome of a restore (`RestoreRead` minus the fixed message).
#[derive(Debug, Clone, PartialEq)]
pub struct RestoreOutcome {
    pub restored_from: String,
    pub database: String,
    pub previous_copy: String,
}

/// The backup/restore file-subsystem seam (W5).
///
/// All three operations go through the SQLite online-backup API so every
/// copy is consistent while the live database stays open in WAL mode; the
/// drill in `docs/testing/backup-restore-drill.md` is the acceptance manual.
/// Errors carry the baseline's codes (`backup_failed`, `backup_not_managed`,
/// …) as `StatusWithCode`.
#[async_trait]
pub trait BackupStore: Send + Sync {
    /// Both backup families, newest first.
    async fn list(&self) -> Result<Vec<BackupRecord>, ApplicationError>;
    /// One online-backup copy plus retention pruning and sidecar cleanup.
    async fn create(&self) -> Result<BackupRecord, ApplicationError>;
    /// The pre-restore safety copy, the in-place restore, the
    /// `backup.restored` audit row (same connection) and pruning.
    async fn restore(&self, name: &str) -> Result<RestoreOutcome, ApplicationError>;
}

/// The auth data seam (W6): users, tenants, refresh tokens, shares.
///
/// Refresh-token rotation is single-use: `revoke_refresh_token_if_active`
/// runs a guarded UPDATE (`revoked_at IS NULL AND expires_at > ?`) and
/// reports whether THIS caller won the race.
#[async_trait]
pub trait AuthRepository: Send + Sync {
    async fn get_user(&self, id: &str) -> Result<Option<UserData>, RepositoryError>;
    async fn find_user_by_email(&self, email: &str) -> Result<Option<UserData>, RepositoryError>;
    async fn insert_user(&self, user: &UserData) -> Result<UserData, RepositoryError>;
    async fn get_default_tenant(&self) -> Result<Option<String>, RepositoryError>;
    async fn get_tenant(&self, id: &str) -> Result<Option<String>, RepositoryError>;
    async fn insert_tenant(&self, name: &str) -> Result<String, RepositoryError>;

    async fn insert_refresh_token(
        &self,
        user_id: &str,
        token_hash: &str,
        expires_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<(), RepositoryError>;
    async fn find_refresh_token(
        &self,
        token_hash: &str,
    ) -> Result<Option<crate::auth::RefreshTokenRow>, RepositoryError>;
    /// The guarded single-use revoke: true when this caller won.
    async fn revoke_refresh_token_if_active(
        &self,
        token_hash: &str,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<bool, RepositoryError>;
    /// Idempotent revoke (logout): any state, no winner semantics.
    async fn revoke_refresh_token_by_hash(
        &self,
        token_hash: &str,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<(), RepositoryError>;

    async fn upsert_share(
        &self,
        owner_id: &str,
        recipient_id: &str,
        resource_type: &str,
        resource_id: &str,
        permission: &str,
    ) -> Result<ShareData, RepositoryError>;
    async fn list_shares_for_user(&self, user_id: &str) -> Result<Vec<ShareData>, RepositoryError>;
    async fn get_share(&self, id: &str) -> Result<Option<ShareData>, RepositoryError>;
    async fn delete_share(&self, id: &str) -> Result<(), RepositoryError>;
}

/// The crypto seam (W6): bcrypt hashing, HS256 access tokens, refresh-token
/// generation. Implemented by the infra adapter; tests can fake it.
#[async_trait]
pub trait AuthSecurity: Send + Sync {
    async fn hash_password(&self, plain: &str) -> String;
    async fn verify_password(&self, plain: &str, hash: &str) -> bool;
    /// `(token, expires_in_seconds)`.
    async fn create_access_token(
        &self,
        user_id: &str,
        tenant_id: &str,
        role: &str,
    ) -> (String, i64);
    async fn new_refresh_token(&self) -> String;
    async fn hash_refresh_token(&self, token: &str) -> String;
    /// The refresh-token lifetime in days (`jwt_refresh_token_expire_days`).
    fn refresh_token_days(&self) -> i64;
}
