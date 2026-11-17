//! Repository ports (the application's outward-facing seam).

use async_trait::async_trait;
use ica_domain::{Candidate, Conversation, Message, Position, SendRecord};

use crate::payload::{
    CandidateFilter, CandidatePatch, ConversationFilter, MessageFilter, NewCandidate,
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

/// A persistence failure. The message is logged and surfaced as a generic
/// 500 to the client, never echoed (it can carry SQL details).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepositoryError {
    message: String,
}

impl RepositoryError {
    pub fn unavailable(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    pub fn message(&self) -> &str {
        &self.message
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
