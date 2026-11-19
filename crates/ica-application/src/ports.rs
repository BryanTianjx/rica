//! Repository ports (the application's outward-facing seam).

use std::collections::BTreeMap;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use ica_domain::{
    Candidate, Conversation, FollowUpPolicy, FollowUpTask, FollowUpTaskStatus, GeneratedDraft,
    Message, Position, SendRecord,
};

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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepositoryError {
    message: String,
    kind: RepositoryErrorKind,
}

/// Which envelope this failure must become.
///
/// The three non-500 cases are genuinely different responses in the baseline
/// and must not be collapsed: `NotFoundError` is a 404, `BusinessRuleError` a
/// 409, `ValidationError` a 422. Storing them as separate data (instead of "a
/// details map means 409") is what lets a 404 and a 422 both carry details and
/// still be told apart.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RepositoryErrorKind {
    /// A persistence failure: 500, message not echoed.
    Unavailable,
    /// The row does not exist: 404.
    NotFound,
    /// A business-rule refusal: 409 with `details`.
    Conflict(BTreeMap<String, String>),
    /// A validation refusal: 422 with `details` and a caller-chosen code.
    ///
    /// The code matters: an import payload that cannot be read back is
    /// `import_validation_error`, not the generic `validation_error`.
    Invalid {
        code: &'static str,
        details: BTreeMap<String, String>,
    },
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
    pub fn invalid(message: impl Into<String>, details: BTreeMap<String, String>) -> Self {
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
        details: BTreeMap<String, String>,
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
                    details: serde_json::json!(details),
                }
            }
            RepositoryErrorKind::Unavailable => crate::error::ApplicationError::Unavailable {
                message: self.message.clone(),
            },
        }
    }
}

impl std::fmt::Display for RepositoryError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for RepositoryError {}

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

    /// Insert one position and its audit event in a single transaction.
    async fn create(&self, new: NewPosition) -> Result<Position, RepositoryError>;

    /// Apply a patch; ``Ok(None)`` means the row does not exist.
    async fn update(
        &self,
        id: &str,
        patch: PositionPatch,
    ) -> Result<Option<Position>, RepositoryError>;

    /// Delete one position; ``Ok(false)`` means the row does not exist.
    ///
    /// The FK is `ON DELETE SET NULL`, so linked candidates keep their row and
    /// lose only the link.
    async fn delete(&self, id: &str) -> Result<bool, RepositoryError>;
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

/// The AI generation port. `ica-application` never talks to a provider: the
/// adapter implements this against whatever backend is configured (W5 wires
/// the real HTTP client; the W3 implementation answers with the baseline's
/// not-configured error, byte-for-byte).
#[async_trait]
pub trait DraftGenerator: Send + Sync {
    async fn generate(
        &self,
        request: crate::ai_context::GenerateDraftRequest,
    ) -> Result<GeneratedDraft, ApplicationError>;
}
