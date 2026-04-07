use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseConnection, DbBackend, EntityTrait,
    QueryFilter, Set, Statement, TransactionTrait,
};
use uuid::Uuid;

use crate::entities::{chat_members, chats};
use crate::errors::AppError;

#[derive(Clone)]
pub struct ChatRepository {
    db: DatabaseConnection,
}

#[derive(Debug, Clone)]
pub struct ChatWithStats {
    pub chat: chats::Model,
    pub member_count: u64,
    pub unread_count: u64,
}

impl ChatRepository {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    pub async fn create_chat_with_members(
        &self,
        name: String,
        chat_type: String,
        members: Vec<Uuid>,
    ) -> Result<chats::Model, AppError> {
        let txn = self.db.begin().await?;
        let now = Utc::now().fixed_offset();

        let chat = chats::ActiveModel {
            id: Set(Uuid::new_v4()),
            name: Set(name),
            chat_type: Set(chat_type),
            created_at: Set(now),
            last_message_at: Set(now),
        }
        .insert(&txn)
        .await?;

        Self::insert_members_with_conn(&txn, chat.id, &members).await?;

        txn.commit().await?;
        Ok(chat)
    }

    pub async fn find_by_id(&self, chat_id: Uuid) -> Result<Option<chats::Model>, AppError> {
        Ok(chats::Entity::find_by_id(chat_id).one(&self.db).await?)
    }

    pub async fn list_user_chats_with_stats(
        &self,
        user_id: Uuid,
    ) -> Result<Vec<ChatWithStats>, AppError> {
        let statement = Statement::from_sql_and_values(
            DbBackend::Postgres,
            r#"
SELECT
  c.id,
  c.name,
  c.chat_type,
  c.created_at,
  c.last_message_at,
  COALESCE(mc.member_count, 0)::BIGINT AS member_count,
  COALESCE((
    SELECT COUNT(*)::BIGINT
    FROM messages m
    WHERE m.chat_id = c.id
      AND m.sender_id <> $1
      AND m.seq > cm_self.last_read_seq
  ), 0)::BIGINT AS unread_count
FROM chats c
JOIN chat_members cm_self
  ON cm_self.chat_id = c.id
  AND cm_self.user_id = $1
LEFT JOIN (
  SELECT chat_id, COUNT(*)::BIGINT AS member_count
  FROM chat_members
  GROUP BY chat_id
) mc
  ON mc.chat_id = c.id
ORDER BY c.last_message_at DESC
"#,
            [user_id.into()],
        );

        self.db
            .query_all(statement)
            .await?
            .into_iter()
            .map(Self::chat_with_stats_from_row)
            .collect::<Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    pub async fn find_chat_with_stats_for_user(
        &self,
        chat_id: Uuid,
        user_id: Uuid,
    ) -> Result<Option<ChatWithStats>, AppError> {
        let statement = Statement::from_sql_and_values(
            DbBackend::Postgres,
            r#"
SELECT
  c.id,
  c.name,
  c.chat_type,
  c.created_at,
  c.last_message_at,
  COALESCE((
    SELECT COUNT(*)::BIGINT
    FROM chat_members cm
    WHERE cm.chat_id = c.id
  ), 0)::BIGINT AS member_count,
  COALESCE((
    SELECT COUNT(*)::BIGINT
    FROM messages m
    WHERE m.chat_id = c.id
      AND m.sender_id <> $1
      AND m.seq > cm_self.last_read_seq
  ), 0)::BIGINT AS unread_count
FROM chats c
JOIN chat_members cm_self
  ON cm_self.chat_id = c.id
  AND cm_self.user_id = $1
WHERE c.id = $2
LIMIT 1
"#,
            [user_id.into(), chat_id.into()],
        );

        self.db
            .query_one(statement)
            .await?
            .map(Self::chat_with_stats_from_row)
            .transpose()
            .map_err(Into::into)
    }

    pub async fn find_or_create_direct_chat(
        &self,
        name: String,
        first_user_id: Uuid,
        second_user_id: Uuid,
    ) -> Result<chats::Model, AppError> {
        let txn = self.db.begin().await?;

        let (lower_user_id, upper_user_id) = if first_user_id <= second_user_id {
            (first_user_id, second_user_id)
        } else {
            (second_user_id, first_user_id)
        };

        txn.query_all(Statement::from_sql_and_values(
            DbBackend::Postgres,
            r#"
SELECT id
FROM users
WHERE id IN ($1, $2)
ORDER BY id
FOR UPDATE
"#,
            [lower_user_id.into(), upper_user_id.into()],
        ))
        .await?;

        if let Some(existing_chat) =
            Self::find_direct_chat_by_members_with_conn(&txn, lower_user_id, upper_user_id).await?
        {
            txn.commit().await?;
            return Ok(existing_chat);
        }

        let now = Utc::now().fixed_offset();
        let chat = chats::ActiveModel {
            id: Set(Uuid::new_v4()),
            name: Set(name),
            chat_type: Set("direct".to_string()),
            created_at: Set(now),
            last_message_at: Set(now),
        }
        .insert(&txn)
        .await?;

        Self::insert_members_with_conn(&txn, chat.id, &[lower_user_id, upper_user_id]).await?;

        txn.commit().await?;
        Ok(chat)
    }

    pub async fn is_member(&self, chat_id: Uuid, user_id: Uuid) -> Result<bool, AppError> {
        let member = chat_members::Entity::find_by_id((chat_id, user_id))
            .one(&self.db)
            .await?;
        Ok(member.is_some())
    }

    pub async fn list_member_ids(&self, chat_id: Uuid) -> Result<Vec<Uuid>, AppError> {
        let members = chat_members::Entity::find()
            .filter(chat_members::Column::ChatId.eq(chat_id))
            .all(&self.db)
            .await?;

        Ok(members.into_iter().map(|m| m.user_id).collect())
    }

    pub async fn mark_read_up_to(
        &self,
        chat_id: Uuid,
        user_id: Uuid,
        seq: i64,
    ) -> Result<(i64, i64, i64), AppError> {
        let txn = self.db.begin().await?;
        let row = txn
            .query_one(Statement::from_sql_and_values(
                DbBackend::Postgres,
                r#"
SELECT last_read_seq
FROM chat_members
WHERE chat_id = $1
  AND user_id = $2
FOR UPDATE
"#,
                [chat_id.into(), user_id.into()],
            ))
            .await?
            .ok_or(AppError::Forbidden)?;

        let old_seq = row.try_get::<i64>("", "last_read_seq").unwrap_or(0);
        let latest_seq = txn
            .query_one(Statement::from_sql_and_values(
                DbBackend::Postgres,
                r#"
SELECT COALESCE(MAX(seq), 0)::BIGINT AS latest_seq
FROM messages
WHERE chat_id = $1
"#,
                [chat_id.into()],
            ))
            .await?
            .and_then(|latest_row| latest_row.try_get::<i64>("", "latest_seq").ok())
            .unwrap_or(0);
        let bounded_seq = clamp_read_seq(seq, latest_seq);
        let new_seq = old_seq.max(bounded_seq);

        let advanced = if new_seq > old_seq {
            txn.query_one(Statement::from_sql_and_values(
                DbBackend::Postgres,
                r#"
SELECT COUNT(*)::BIGINT AS advanced
FROM messages
WHERE chat_id = $1
  AND sender_id <> $2
  AND seq > $3
  AND seq <= $4
"#,
                [
                    chat_id.into(),
                    user_id.into(),
                    old_seq.into(),
                    new_seq.into(),
                ],
            ))
            .await?
            .and_then(|count_row| count_row.try_get::<i64>("", "advanced").ok())
            .unwrap_or(0)
        } else {
            0
        };

        txn.execute(Statement::from_sql_and_values(
            DbBackend::Postgres,
            r#"
UPDATE chat_members
SET last_read_seq = $3
WHERE chat_id = $1
  AND user_id = $2
  AND last_read_seq < $3
"#,
            [chat_id.into(), user_id.into(), new_seq.into()],
        ))
        .await?;

        txn.commit().await?;
        Ok((old_seq, new_seq, advanced))
    }

    pub async fn insert_members(&self, chat_id: Uuid, members: &[Uuid]) -> Result<(), AppError> {
        Self::insert_members_with_conn(&self.db, chat_id, members)
            .await
            .map_err(Into::into)
    }

    async fn insert_members_with_conn<C: ConnectionTrait>(
        conn: &C,
        chat_id: Uuid,
        members: &[Uuid],
    ) -> Result<(), sea_orm::DbErr> {
        if members.is_empty() {
            return Ok(());
        }

        let joined_at = Utc::now().fixed_offset();
        let member_models = members
            .iter()
            .map(|&user_id| chat_members::ActiveModel {
                chat_id: Set(chat_id),
                user_id: Set(user_id),
                joined_at: Set(joined_at),
                last_read_seq: Set(0),
            })
            .collect::<Vec<_>>();

        chat_members::Entity::insert_many(member_models)
            .exec(conn)
            .await?;

        Ok(())
    }

    async fn find_direct_chat_by_members_with_conn<C: ConnectionTrait>(
        conn: &C,
        first_user_id: Uuid,
        second_user_id: Uuid,
    ) -> Result<Option<chats::Model>, sea_orm::DbErr> {
        conn.query_one(Statement::from_sql_and_values(
            DbBackend::Postgres,
            r#"
SELECT
  c.id,
  c.name,
  c.chat_type,
  c.created_at,
  c.last_message_at
FROM chats c
JOIN chat_members cm_first
  ON cm_first.chat_id = c.id
  AND cm_first.user_id = $1
JOIN chat_members cm_second
  ON cm_second.chat_id = c.id
  AND cm_second.user_id = $2
WHERE c.chat_type = 'direct'
  AND (
    SELECT COUNT(*)::BIGINT
    FROM chat_members cm_count
    WHERE cm_count.chat_id = c.id
  ) = 2
ORDER BY c.created_at ASC, c.id ASC
LIMIT 1
"#,
            [first_user_id.into(), second_user_id.into()],
        ))
        .await?
        .map(Self::chat_model_from_row)
        .transpose()
    }

    fn chat_model_from_row(row: sea_orm::QueryResult) -> Result<chats::Model, sea_orm::DbErr> {
        Ok(chats::Model {
            id: row.try_get("", "id")?,
            name: row.try_get("", "name")?,
            chat_type: row.try_get("", "chat_type")?,
            created_at: row.try_get("", "created_at")?,
            last_message_at: row.try_get("", "last_message_at")?,
        })
    }

    fn chat_with_stats_from_row(
        row: sea_orm::QueryResult,
    ) -> Result<ChatWithStats, sea_orm::DbErr> {
        let member_count = row.try_get::<i64>("", "member_count").unwrap_or(0) as u64;
        let unread_count = row.try_get::<i64>("", "unread_count").unwrap_or(0) as u64;
        Ok(ChatWithStats {
            chat: Self::chat_model_from_row(row)?,
            member_count,
            unread_count,
        })
    }
}

fn clamp_read_seq(requested_seq: i64, latest_seq: i64) -> i64 {
    requested_seq.max(0).min(latest_seq.max(0))
}

#[cfg(test)]
mod tests {
    use super::clamp_read_seq;

    #[test]
    fn clamps_future_read_cursor_to_latest_message_seq() {
        assert_eq!(clamp_read_seq(99, 7), 7);
    }

    #[test]
    fn clamps_empty_chat_to_zero() {
        assert_eq!(clamp_read_seq(99, 0), 0);
    }

    #[test]
    fn clamps_negative_requested_seq_to_zero() {
        assert_eq!(clamp_read_seq(-5, 7), 0);
    }

    #[test]
    fn keeps_in_range_seq_unchanged() {
        assert_eq!(clamp_read_seq(5, 7), 5);
    }
}
