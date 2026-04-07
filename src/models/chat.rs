use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chat {
    pub id: Uuid,
    pub name: String,
    pub chat_type: String,
    pub created_at: DateTime<Utc>,
    pub last_message_at: DateTime<Utc>,
    pub member_count: u64,
    pub unread_count: u64,
}

impl Chat {
    pub fn from_model(
        value: crate::entities::chats::Model,
        member_count: u64,
        unread_count: u64,
    ) -> Self {
        Self {
            id: value.id,
            name: value.name,
            chat_type: value.chat_type,
            created_at: value.created_at.with_timezone(&Utc),
            last_message_at: value.last_message_at.with_timezone(&Utc),
            member_count,
            unread_count,
        }
    }
}
