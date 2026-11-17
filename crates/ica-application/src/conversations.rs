//! Conversation, message and timeline use cases.

use std::collections::BTreeMap;

use ica_domain::{Message, Page, TimelineEntry};

use crate::error::ApplicationError;
use crate::payload::{ConversationFilter, MessageFilter, NewConversation, NewMessage};
use crate::ports::{
    CandidateRepository, ConversationRepository, MessageRepository, RepositoryError,
};

/// A validated ``GET /api/conversations`` request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListConversations {
    pub candidate_id: Option<String>,
    pub offset: usize,
    pub limit: usize,
}

/// A validated ``GET /api/messages`` request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListMessages {
    pub candidate_id: Option<String>,
    pub conversation_id: Option<String>,
    pub offset: usize,
    pub limit: usize,
}

fn unavailable(error: RepositoryError) -> ApplicationError {
    ApplicationError::Unavailable {
        message: error.message().to_string(),
    }
}

/// Conversations, `last_message_at DESC NULLS LAST`, with the real total.
///
/// `message_count` comes from one grouped count for the whole window (OPT-1),
/// not one query per conversation.
pub async fn list_conversations(
    repository: &dyn ConversationRepository,
    query: ListConversations,
) -> Result<Page<ica_domain::Conversation>, ApplicationError> {
    let filter = ConversationFilter {
        candidate_id: query.candidate_id,
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

/// Open a conversation for a candidate and audit `conversation.created`.
pub async fn create_conversation(
    candidates: &dyn CandidateRepository,
    repository: &dyn ConversationRepository,
    new: NewConversation,
) -> Result<ica_domain::Conversation, ApplicationError> {
    crate::candidates::get_candidate(candidates, &new.candidate_id).await?;
    repository.create(new).await.map_err(unavailable)
}

/// Messages in time order (`message_time ASC NULLS LAST, created_at ASC`).
///
/// `total` is the real number of matching rows, independent of `limit`/`offset`
/// — IMP-1. The Python baseline truncates it to `len(messages[0 .. limit+offset])`.
pub async fn list_messages(
    repository: &dyn MessageRepository,
    query: ListMessages,
) -> Result<Page<Message>, ApplicationError> {
    let filter = MessageFilter {
        candidate_id: query.candidate_id,
        conversation_id: query.conversation_id,
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

/// Record one message, opening or reusing the candidate's conversation.
///
/// The coupling rules are the baseline's:
/// * an explicit `conversation_id` must exist (404) and belong to the candidate
///   (422 with `conversation_id` + `candidate_id` in `details`);
/// * without one, the candidate's active conversation
///   (`last_message_at DESC NULLS LAST`, first row) is reused, or a new
///   `channel='boss'` conversation is opened — silently, with no
///   `conversation.created` audit event, because the baseline only audits the
///   explicit `POST /api/conversations`.
pub async fn create_message(
    candidates: &dyn CandidateRepository,
    conversations: &dyn ConversationRepository,
    messages: &dyn MessageRepository,
    new: NewMessage,
) -> Result<Message, ApplicationError> {
    let candidate = crate::candidates::get_candidate(candidates, &new.candidate_id).await?;

    let conversation_id = match new.conversation_id.clone() {
        Some(conversation_id) => {
            let conversation = conversations
                .find(&conversation_id)
                .await
                .map_err(unavailable)?
                .ok_or_else(|| ApplicationError::not_found("Conversation", &conversation_id))?;
            if conversation.candidate_id != candidate.id {
                let mut details = BTreeMap::new();
                details.insert("conversation_id".to_string(), conversation_id);
                details.insert("candidate_id".to_string(), candidate.id.clone());
                return Err(ApplicationError::validation(
                    "Conversation does not belong to the given candidate",
                    details,
                ));
            }
            conversation.id
        }
        None => match conversations
            .find_active_for_candidate(&candidate.id)
            .await
            .map_err(unavailable)?
        {
            Some(conversation) => conversation.id,
            None => {
                conversations
                    .create_implicit(&candidate.id, "boss")
                    .await
                    .map_err(unavailable)?
                    .id
            }
        },
    };

    messages
        .create(new, &conversation_id)
        .await
        .map_err(unavailable)
}

/// The candidate's message timeline.
///
/// The candidate is resolved first: an empty list is not a refusal, so a
/// missing candidate must 404 instead of returning `[]`.
pub async fn candidate_timeline(
    candidates: &dyn CandidateRepository,
    messages: &dyn MessageRepository,
    candidate_id: &str,
    limit: usize,
) -> Result<Vec<TimelineEntry>, ApplicationError> {
    crate::candidates::get_candidate(candidates, candidate_id).await?;
    let filter = MessageFilter {
        candidate_id: Some(candidate_id.to_string()),
        conversation_id: None,
    };
    let (rows, _total) = messages.list(filter, 0, limit).await.map_err(unavailable)?;
    Ok(rows
        .into_iter()
        .map(|message| TimelineEntry {
            id: message.id,
            kind: "message",
            sender_type: message.sender_type,
            content: message.content,
            message_time: message.message_time,
            source: Some(message.source),
            status: None,
            created_at: Some(message.created_at),
        })
        .collect())
}
