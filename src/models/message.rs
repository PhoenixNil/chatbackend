use crate::entities::messages::Model as MessageEntity;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const TEXT_MESSAGE_TYPE: &str = "text";
pub const IMAGE_MESSAGE_TYPE: &str = "image";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: Uuid,
    pub chat_id: Uuid,
    pub seq: i64,
    pub sender_id: Uuid,
    pub body: String,
    pub message_type: String,
    pub media_url: Option<String>,
    pub media_width: Option<i32>,
    pub media_height: Option<i32>,
    pub media_size_bytes: Option<i64>,
    pub media_content_type: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl From<MessageEntity> for Message {
    fn from(value: MessageEntity) -> Self {
        Self {
            id: value.id,
            chat_id: value.chat_id,
            seq: value.seq,
            sender_id: value.sender_id,
            body: value.body,
            message_type: value.message_type,
            media_url: value.media_url,
            media_width: value.media_width,
            media_height: value.media_height,
            media_size_bytes: value.media_size_bytes,
            media_content_type: value.media_content_type,
            created_at: value.created_at.with_timezone(&Utc),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageSearchResult {
    pub id: Uuid,
    pub chat_id: Uuid,
    pub seq: i64,
    pub sender_id: Uuid,
    pub body: String,
    pub message_type: String,
    pub media_url: Option<String>,
    pub media_width: Option<i32>,
    pub media_height: Option<i32>,
    pub media_size_bytes: Option<i64>,
    pub media_content_type: Option<String>,
    pub created_at: DateTime<Utc>,
    pub sender_username: String,
    pub sender_avatar_url: Option<String>,
}
