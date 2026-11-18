//! SQLite implementation of the conversation and message ports.
//!
//! Two parity details live here and nowhere else:
//!
//! * `message_count` is one grouped aggregate for the whole page (OPT-1). The
//!   Python baseline counts messages per conversation with `limit=1000`, so the
//!   aggregate caps at 1000 as well — the number is identical, it just costs one
//!   query instead of one per row.
//! * A message write moves `conversation.last_message_at` (and `updated_at`,
//!   which the ORM's `onupdate` bumps) in the *same* transaction, because a
//!   message whose conversation still shows "no activity" is a visible defect.

use async_trait::async_trait;
use chrono::Utc;
use ica_application::{
    ConversationFilter, ConversationRepository, MessageFilter, MessageRepository, NewConversation,
    NewMessage, RepositoryError,
};
use ica_domain::{content_hash, Conversation, Message};
use sqlx::{FromRow, SqliteConnection};

use crate::audit::{insert_audit, AuditEntry};
use crate::store::{
    offset_bind, repo_error, to_sql_timestamp, where_clause, SqliteStore, DEFAULT_TENANT_ID,
    LOCAL_ACTOR, LOCAL_OWNER_ID,
};
use serde_json::json;

const SELECT_COLUMNS: &str = concat!(
    "SELECT c.id, c.candidate_id, c.channel, c.title, c.is_active, c.last_message_at,",
    " c.created_at, c.updated_at, p.id AS position_id, p.name AS position_name,",
    " CASE WHEN COALESCE(m.cnt, 0) > 1000 THEN 1000 ELSE COALESCE(m.cnt, 0) END AS message_count",
    " FROM conversations c",
    " LEFT JOIN candidates cand ON cand.id = c.candidate_id",
    " LEFT JOIN positions p ON p.id = cand.position_id",
    " LEFT JOIN (SELECT conversation_id, COUNT(*) AS cnt FROM messages",
    "            GROUP BY conversation_id) m ON m.conversation_id = c.id",
);

/// `last_message_at DESC NULLS LAST` without depending on SQLite ≥ 3.30.
const ORDER_BY: &str = " ORDER BY (c.last_message_at IS NULL), c.last_message_at DESC";

#[derive(Debug, FromRow)]
struct ConversationRow {
    id: String,
    candidate_id: String,
    channel: String,
    title: Option<String>,
    is_active: i64,
    last_message_at: Option<String>,
    created_at: String,
    updated_at: String,
    position_id: Option<String>,
    position_name: Option<String>,
    message_count: i64,
}

impl ConversationRow {
    fn into_conversation(self) -> Result<Conversation, RepositoryError> {
        // Decode every timestamp before moving any field out of `self`.
        let last_message_at = match self.last_message_at.as_deref() {
            Some(raw) => Some(parse_conversation_time(raw, "last_message_at", &self.id)?),
            None => None,
        };
        let created_at = parse_conversation_time(&self.created_at, "created_at", &self.id)?;
        let updated_at = parse_conversation_time(&self.updated_at, "updated_at", &self.id)?;
        Ok(Conversation {
            id: self.id,
            candidate_id: self.candidate_id,
            channel: self.channel,
            title: self.title,
            is_active: self.is_active != 0,
            last_message_at,
            created_at,
            updated_at,
            message_count: self.message_count,
            position_id: self.position_id,
            position_name: self.position_name,
        })
    }
}

/// Decode one stored conversation timestamp, naming the row on failure.
fn parse_conversation_time(
    raw: &str,
    column: &str,
    id: &str,
) -> Result<chrono::DateTime<chrono::Utc>, RepositoryError> {
    crate::time::parse_sqlite_datetime(raw).map_err(|error| {
        RepositoryError::unavailable(format!("conversations.{column} for id '{id}': {error}"))
    })
}

async fn fetch_conversations(
    connection: &mut SqliteConnection,
    sql: &str,
    binds: &[String],
    limit: Option<(usize, usize)>,
) -> Result<Vec<Conversation>, RepositoryError> {
    let mut query = sqlx::query_as::<_, ConversationRow>(sql);
    for value in binds {
        query = query.bind(value.clone());
    }
    if let Some((limit, offset)) = limit {
        query = query.bind(limit as i64).bind(offset_bind(offset));
    }
    let rows = query
        .fetch_all(&mut *connection)
        .await
        .map_err(repo_error)?;
    rows.into_iter()
        .map(ConversationRow::into_conversation)
        .collect()
}

#[async_trait]
impl ConversationRepository for SqliteStore {
    async fn list(
        &self,
        filter: ConversationFilter,
        offset: usize,
        limit: usize,
    ) -> Result<(Vec<Conversation>, usize), RepositoryError> {
        let mut conditions: Vec<&str> = Vec::new();
        let mut binds: Vec<String> = Vec::new();
        if let Some(candidate_id) = filter.candidate_id.as_deref() {
            conditions.push("c.candidate_id = ?");
            binds.push(candidate_id.to_string());
        }
        let where_sql = where_clause(&conditions);

        let count_sql = format!("SELECT COUNT(*) FROM conversations c{where_sql}");
        let mut count_query = sqlx::query_scalar::<_, i64>(&count_sql);
        for value in &binds {
            count_query = count_query.bind(value.clone());
        }
        let total: i64 = count_query
            .fetch_one(self.pool())
            .await
            .map_err(repo_error)?;

        let sql = format!("{SELECT_COLUMNS}{where_sql}{ORDER_BY} LIMIT ? OFFSET ?");
        let mut connection = self.pool().acquire().await.map_err(repo_error)?;
        let items =
            fetch_conversations(&mut connection, &sql, &binds, Some((limit, offset))).await?;
        Ok((items, total as usize))
    }

    async fn find(&self, id: &str) -> Result<Option<Conversation>, RepositoryError> {
        let sql = format!("{SELECT_COLUMNS} WHERE c.id = ?");
        let mut connection = self.pool().acquire().await.map_err(repo_error)?;
        let mut rows = fetch_conversations(&mut connection, &sql, &[id.to_string()], None).await?;
        Ok(rows.pop())
    }

    async fn find_active_for_candidate(
        &self,
        candidate_id: &str,
    ) -> Result<Option<Conversation>, RepositoryError> {
        let sql = format!(
            "{SELECT_COLUMNS} WHERE c.candidate_id = ? AND c.is_active = 1{ORDER_BY} LIMIT 1"
        );
        let mut connection = self.pool().acquire().await.map_err(repo_error)?;
        let mut rows =
            fetch_conversations(&mut connection, &sql, &[candidate_id.to_string()], None).await?;
        Ok(rows.pop())
    }

    async fn create(&self, new: NewConversation) -> Result<Conversation, RepositoryError> {
        let now = to_sql_timestamp(&Utc::now());
        let id = crate::store::new_uuid();

        let mut tx = self.pool().begin().await.map_err(repo_error)?;
        insert_conversation(
            &mut tx,
            &id,
            &new.candidate_id,
            &new.channel,
            new.title.as_deref(),
            &now,
        )
        .await?;
        insert_audit(
            &mut tx,
            AuditEntry::new(
                "conversation.created",
                "conversation",
                &id,
                "Opened conversation",
            ),
        )
        .await
        .map_err(repo_error)?;
        tx.commit().await.map_err(repo_error)?;

        self.find(&id)
            .await?
            .ok_or_else(|| RepositoryError::unavailable("inserted conversation disappeared"))
    }

    async fn create_implicit(
        &self,
        candidate_id: &str,
        channel: &str,
    ) -> Result<Conversation, RepositoryError> {
        let now = to_sql_timestamp(&Utc::now());
        let id = crate::store::new_uuid();

        let mut tx = self.pool().begin().await.map_err(repo_error)?;
        insert_conversation(&mut tx, &id, candidate_id, channel, None, &now).await?;
        tx.commit().await.map_err(repo_error)?;

        self.find(&id)
            .await?
            .ok_or_else(|| RepositoryError::unavailable("inserted conversation disappeared"))
    }
}

/// Insert one conversation row on an open transaction.
async fn insert_conversation(
    connection: &mut SqliteConnection,
    id: &str,
    candidate_id: &str,
    channel: &str,
    title: Option<&str>,
    now: &str,
) -> Result<(), RepositoryError> {
    sqlx::query(concat!(
        "INSERT INTO conversations (id, candidate_id, channel, title, is_active,",
        " last_message_at, created_at, updated_at, owner_id, tenant_id)",
        " VALUES (?, ?, ?, ?, 1, NULL, ?, ?, ?, ?)"
    ))
    .bind(id)
    .bind(candidate_id)
    .bind(channel)
    .bind(title)
    .bind(now)
    .bind(now)
    .bind(LOCAL_OWNER_ID)
    .bind(DEFAULT_TENANT_ID)
    .execute(connection)
    .await
    .map_err(repo_error)?;
    Ok(())
}

// ── Messages ───────────────────────────────────────────────────────────────

const MESSAGE_COLUMNS: &str = concat!(
    "SELECT id, candidate_id, conversation_id, sender_type, content, message_time,",
    " channel, source, created_at FROM messages"
);

/// `message_time ASC NULLS LAST, created_at ASC`.
const MESSAGE_ORDER: &str = " ORDER BY (message_time IS NULL), message_time ASC, created_at ASC";

#[derive(Debug, FromRow)]
struct MessageRow {
    id: String,
    candidate_id: String,
    conversation_id: String,
    sender_type: Option<String>,
    content: String,
    message_time: Option<String>,
    channel: String,
    source: String,
    created_at: String,
}

impl MessageRow {
    fn into_message(self) -> Result<Message, RepositoryError> {
        let message_time = match self.message_time.as_deref() {
            Some(raw) => Some(crate::time::parse_sqlite_datetime(raw).map_err(|error| {
                RepositoryError::unavailable(format!(
                    "messages.message_time for id '{}': {error}",
                    self.id
                ))
            })?),
            None => None,
        };
        let created_at = crate::time::parse_sqlite_datetime(&self.created_at).map_err(|error| {
            RepositoryError::unavailable(format!(
                "messages.created_at for id '{}': {error}",
                self.id
            ))
        })?;
        Ok(Message {
            id: self.id,
            candidate_id: self.candidate_id,
            conversation_id: self.conversation_id,
            sender_type: self.sender_type,
            content: self.content,
            message_time,
            channel: self.channel,
            source: self.source,
            created_at,
        })
    }
}

#[async_trait]
impl MessageRepository for SqliteStore {
    async fn list(
        &self,
        filter: MessageFilter,
        offset: usize,
        limit: usize,
    ) -> Result<(Vec<Message>, usize), RepositoryError> {
        let mut conditions: Vec<&str> = Vec::new();
        let mut binds: Vec<String> = Vec::new();
        if let Some(candidate_id) = filter.candidate_id.as_deref() {
            conditions.push("candidate_id = ?");
            binds.push(candidate_id.to_string());
        }
        if let Some(conversation_id) = filter.conversation_id.as_deref() {
            conditions.push("conversation_id = ?");
            binds.push(conversation_id.to_string());
        }
        let where_sql = where_clause(&conditions);

        // IMP-1/IMP-3: the unconditional count is the *real* number of matching
        // rows. The baseline derives `total` from the window it just fetched
        // (`limit + offset`), so a client cannot compute the page count.
        let count_sql = format!("SELECT COUNT(*) FROM messages{where_sql}");
        let mut count_query = sqlx::query_scalar::<_, i64>(&count_sql);
        for value in &binds {
            count_query = count_query.bind(value.clone());
        }
        let total: i64 = count_query
            .fetch_one(self.pool())
            .await
            .map_err(repo_error)?;

        let sql = format!("{MESSAGE_COLUMNS}{where_sql}{MESSAGE_ORDER} LIMIT ? OFFSET ?");
        let mut query = sqlx::query_as::<_, MessageRow>(&sql);
        for value in &binds {
            query = query.bind(value.clone());
        }
        let rows = query
            .bind(limit as i64)
            .bind(offset_bind(offset))
            .fetch_all(self.pool())
            .await
            .map_err(repo_error)?;

        let items = rows
            .into_iter()
            .map(MessageRow::into_message)
            .collect::<Result<Vec<_>, _>>()?;
        Ok((items, total as usize))
    }

    async fn create(
        &self,
        new: NewMessage,
        conversation_id: &str,
    ) -> Result<Message, RepositoryError> {
        let now = to_sql_timestamp(&Utc::now());
        let id = crate::store::new_uuid();
        let hash = content_hash(&new.content);
        let message_time = new.message_time.map(|value| to_sql_timestamp(&value));
        // `conversation.last_message_at = message.message_time or now(UTC)`.
        let last_message_at = message_time.clone().unwrap_or_else(|| now.clone());
        let sender_type = new.sender_type.map(|value| value.as_str());

        let mut tx = self.pool().begin().await.map_err(repo_error)?;
        sqlx::query(concat!(
            "INSERT INTO messages (id, candidate_id, conversation_id, sender_type, content,",
            " message_time, channel, source, content_hash, created_at, updated_at, owner_id,",
            " tenant_id) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
        ))
        .bind(&id)
        .bind(&new.candidate_id)
        .bind(conversation_id)
        .bind(sender_type)
        .bind(&new.content)
        .bind(&message_time)
        .bind(&new.channel)
        .bind(new.source.as_str())
        .bind(&hash)
        .bind(&now)
        .bind(&now)
        .bind(LOCAL_OWNER_ID)
        .bind(DEFAULT_TENANT_ID)
        .execute(&mut *tx)
        .await
        .map_err(repo_error)?;

        sqlx::query("UPDATE conversations SET last_message_at = ?, updated_at = ? WHERE id = ?")
            .bind(&last_message_at)
            .bind(&now)
            .bind(conversation_id)
            .execute(&mut *tx)
            .await
            .map_err(repo_error)?;

        insert_audit(
            &mut tx,
            AuditEntry::new("message.created", "message", &id, "Recorded a message")
                .with_metadata(json!({ "source": new.source.as_str() })),
        )
        .await
        .map_err(repo_error)?;
        tx.commit().await.map_err(repo_error)?;

        // `actor` is not part of a message row; the constant is referenced here
        // so the local-actor rule stays visible next to the write it describes.
        debug_assert_eq!(LOCAL_ACTOR, "local_user");

        Ok(Message {
            id,
            candidate_id: new.candidate_id,
            conversation_id: conversation_id.to_string(),
            sender_type: sender_type.map(str::to_string),
            content: new.content,
            message_time: new.message_time,
            channel: new.channel,
            source: new.source.as_str().to_string(),
            created_at: crate::time::parse_sqlite_datetime(&now)
                .map_err(RepositoryError::unavailable)?,
        })
    }

    /// The AI context builder's read (W3): the candidate's messages
    /// oldest-first, capped — `message_time ASC NULLS LAST, created_at ASC`.
    async fn context_messages(
        &self,
        candidate_id: &str,
        limit: i64,
    ) -> Result<Vec<Message>, RepositoryError> {
        crate::drafts::context_messages(self.pool(), candidate_id, limit).await
    }
}
