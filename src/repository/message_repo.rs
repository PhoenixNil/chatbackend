use chrono::{DateTime, Utc};
use sea_orm::{ConnectionTrait, DatabaseConnection, DbBackend, Statement};
use uuid::Uuid;

use crate::entities::messages;
use crate::errors::AppError;
use crate::models::message::MessageSearchResult;

#[derive(Clone)]
pub struct MessageRepository {
    db: DatabaseConnection,
}

#[derive(Debug, Clone)]
pub struct NewMessageRecord {
    pub chat_id: Uuid,
    pub sender_id: Uuid,
    pub body: String,
    pub message_type: String,
    pub media_url: Option<String>,
    pub media_width: Option<i32>,
    pub media_height: Option<i32>,
    pub media_size_bytes: Option<i64>,
    pub media_content_type: Option<String>,
}

impl MessageRepository {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    pub async fn insert_message_and_touch_chat(
        &self,
        record: NewMessageRecord,
    ) -> Result<messages::Model, AppError> {
        let message_id = Uuid::new_v4();
        let statement = Statement::from_sql_and_values(
            DbBackend::Postgres,
            r#"
WITH ins AS (
  INSERT INTO messages (
    id,
    chat_id,
    sender_id,
    body,
    message_type,
    media_url,
    media_width,
    media_height,
    media_size_bytes,
    media_content_type,
    created_at
  )
  VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, NOW())
  RETURNING
    id,
    chat_id,
    sender_id,
    body,
    message_type,
    media_url,
    media_width,
    media_height,
    media_size_bytes,
    media_content_type,
    created_at,
    seq
), upd AS (
  UPDATE chats c
  SET last_message_at = GREATEST(c.last_message_at, ins.created_at)
  FROM ins
  WHERE c.id = ins.chat_id
  RETURNING 1
)
SELECT
  ins.id,
  ins.chat_id,
  ins.sender_id,
  ins.body,
  ins.message_type,
  ins.media_url,
  ins.media_width,
  ins.media_height,
  ins.media_size_bytes,
  ins.media_content_type,
  ins.created_at,
  ins.seq
FROM ins
JOIN upd ON TRUE
"#,
            [
                message_id.into(),
                record.chat_id.into(),
                record.sender_id.into(),
                record.body.into(),
                record.message_type.into(),
                record.media_url.into(),
                record.media_width.into(),
                record.media_height.into(),
                record.media_size_bytes.into(),
                record.media_content_type.into(),
            ],
        );

        let row = self.db.query_one(statement).await?.ok_or_else(|| {
            AppError::Internal(
                "message insert completed without returning the inserted row".to_string(),
            )
        })?;

        Self::message_model_from_row(row).map_err(Into::into)
    }

    pub async fn find_seq_by_id(&self, message_id: Uuid) -> Result<Option<(Uuid, i64)>, AppError> {
        let row = self
            .db
            .query_one(Statement::from_sql_and_values(
                DbBackend::Postgres,
                r#"SELECT chat_id, seq FROM messages WHERE id = $1"#,
                [message_id.into()],
            ))
            .await?;

        match row {
            Some(r) => Ok(Some((r.try_get("", "chat_id")?, r.try_get("", "seq")?))),
            None => Ok(None),
        }
    }

    pub async fn list_messages(
        &self,
        user_id: Uuid,
        chat_id: Uuid,
        before: Option<DateTime<Utc>>,
        limit: u64,
    ) -> Result<Option<Vec<messages::Model>>, AppError> {
        let statement = if let Some(before_ts) = before {
            Statement::from_sql_and_values(
                DbBackend::Postgres,
                r#"
WITH membership AS (
  SELECT 1
  FROM chat_members cm
  WHERE cm.chat_id = $1
    AND cm.user_id = $2
)
SELECT
  m.id,
  m.chat_id,
  m.seq,
  m.sender_id,
  m.body,
  m.message_type,
  m.media_url,
  m.media_width,
  m.media_height,
  m.media_size_bytes,
  m.media_content_type,
  m.created_at,
  m.seq
FROM membership
LEFT JOIN LATERAL (
  SELECT
    id,
    chat_id,
    sender_id,
    body,
    message_type,
    media_url,
    media_width,
    media_height,
    media_size_bytes,
    media_content_type,
    created_at,
    seq
  FROM messages
  WHERE chat_id = $1
    AND created_at < $3
  ORDER BY created_at DESC
  LIMIT $4
) m ON TRUE
"#,
                [
                    chat_id.into(),
                    user_id.into(),
                    before_ts.into(),
                    limit.into(),
                ],
            )
        } else {
            Statement::from_sql_and_values(
                DbBackend::Postgres,
                r#"
WITH membership AS (
  SELECT 1
  FROM chat_members cm
  WHERE cm.chat_id = $1
    AND cm.user_id = $2
)
SELECT
  m.id,
  m.chat_id,
  m.seq,
  m.sender_id,
  m.body,
  m.message_type,
  m.media_url,
  m.media_width,
  m.media_height,
  m.media_size_bytes,
  m.media_content_type,
  m.created_at,
  m.seq
FROM membership
LEFT JOIN LATERAL (
  SELECT
    id,
    chat_id,
    sender_id,
    body,
    message_type,
    media_url,
    media_width,
    media_height,
    media_size_bytes,
    media_content_type,
    created_at,
    seq
  FROM messages
  WHERE chat_id = $1
  ORDER BY created_at DESC
  LIMIT $3
) m ON TRUE
"#,
                [chat_id.into(), user_id.into(), limit.into()],
            )
        };

        let rows = self.db.query_all(statement).await?;
        if rows.is_empty() {
            return Ok(None);
        }

        let mut messages = Vec::with_capacity(rows.len());
        for row in rows {
            if row.try_get::<Option<Uuid>>("", "id")?.is_none() {
                continue;
            }

            messages.push(Self::message_model_from_row(row)?);
        }

        Ok(Some(messages))
    }

    pub async fn search_messages(
        &self,
        chat_id: Uuid,
        query: &str,
        before: Option<DateTime<Utc>>,
        limit: u64,
    ) -> Result<Vec<MessageSearchResult>, AppError> {
        let pattern = build_ilike_contains_pattern(query);
        let statement = if let Some(before_ts) = before {
            Statement::from_sql_and_values(
                DbBackend::Postgres,
                r#"
SELECT
  m.id,
  m.chat_id,
  m.seq,
  m.sender_id,
  m.body,
  m.message_type,
  m.media_url,
  m.media_width,
  m.media_height,
  m.media_size_bytes,
  m.media_content_type,
  m.created_at,
  u.username AS sender_username,
  u.avatar_url AS sender_avatar_url
FROM messages m
JOIN users u
  ON u.id = m.sender_id
WHERE m.chat_id = $1
  AND m.body ILIKE $2 ESCAPE '\'
  AND m.created_at < $3
ORDER BY m.created_at DESC, m.id DESC
LIMIT $4
"#,
                [
                    chat_id.into(),
                    pattern.into(),
                    before_ts.into(),
                    limit.into(),
                ],
            )
        } else {
            Statement::from_sql_and_values(
                DbBackend::Postgres,
                r#"
SELECT
  m.id,
  m.chat_id,
  m.seq,
  m.sender_id,
  m.body,
  m.message_type,
  m.media_url,
  m.media_width,
  m.media_height,
  m.media_size_bytes,
  m.media_content_type,
  m.created_at,
  u.username AS sender_username,
  u.avatar_url AS sender_avatar_url
FROM messages m
JOIN users u
  ON u.id = m.sender_id
WHERE m.chat_id = $1
  AND m.body ILIKE $2 ESCAPE '\'
ORDER BY m.created_at DESC, m.id DESC
LIMIT $3
"#,
                [chat_id.into(), pattern.into(), limit.into()],
            )
        };

        self.db
            .query_all(statement)
            .await?
            .into_iter()
            .map(Self::search_result_from_row)
            .collect::<Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    fn message_model_from_row(
        row: sea_orm::QueryResult,
    ) -> Result<messages::Model, sea_orm::DbErr> {
        Ok(messages::Model {
            id: row.try_get("", "id")?,
            chat_id: row.try_get("", "chat_id")?,
            sender_id: row.try_get("", "sender_id")?,
            body: row.try_get("", "body")?,
            message_type: row.try_get("", "message_type")?,
            media_url: row.try_get("", "media_url")?,
            media_width: row.try_get("", "media_width")?,
            media_height: row.try_get("", "media_height")?,
            media_size_bytes: row.try_get("", "media_size_bytes")?,
            media_content_type: row.try_get("", "media_content_type")?,
            created_at: row.try_get("", "created_at")?,
            seq: row.try_get("", "seq")?,
        })
    }

    fn search_result_from_row(
        row: sea_orm::QueryResult,
    ) -> Result<MessageSearchResult, sea_orm::DbErr> {
        Ok(MessageSearchResult {
            id: row.try_get("", "id")?,
            chat_id: row.try_get("", "chat_id")?,
            seq: row.try_get("", "seq")?,
            sender_id: row.try_get("", "sender_id")?,
            body: row.try_get("", "body")?,
            message_type: row.try_get("", "message_type")?,
            media_url: row.try_get("", "media_url")?,
            media_width: row.try_get("", "media_width")?,
            media_height: row.try_get("", "media_height")?,
            media_size_bytes: row.try_get("", "media_size_bytes")?,
            media_content_type: row.try_get("", "media_content_type")?,
            created_at: row
                .try_get::<chrono::DateTime<chrono::FixedOffset>>("", "created_at")?
                .with_timezone(&Utc),
            sender_username: row.try_get("", "sender_username")?,
            sender_avatar_url: row.try_get("", "sender_avatar_url")?,
        })
    }
}

fn build_ilike_contains_pattern(query: &str) -> String {
    let mut pattern = String::with_capacity(query.len() + 2);
    pattern.push('%');

    for ch in query.chars() {
        match ch {
            '%' | '_' | '\\' => {
                pattern.push('\\');
                pattern.push(ch);
            }
            _ => pattern.push(ch),
        }
    }

    pattern.push('%');
    pattern
}

#[cfg(test)]
mod tests {
    use super::build_ilike_contains_pattern;

    #[test]
    fn ilike_pattern_wraps_queries_for_contains_matching() {
        assert_eq!(build_ilike_contains_pattern("hello"), "%hello%");
    }

    #[test]
    fn ilike_pattern_escapes_wildcard_characters() {
        assert_eq!(
            build_ilike_contains_pattern(r"100%_done\ready"),
            r"%100\%\_done\\ready%",
        );
    }
}
