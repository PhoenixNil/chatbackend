use axum::extract::ws::Message;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMsg {
    SendMessage {
        chat_id: Uuid,
        client_msg_id: String,
        body: String,
    },
    MarkRead {
        message_id: Uuid,
    },
    SyncPresence,
    Ping,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnreadReason {
    //UnreadReason 是告诉客户端： 「这次未读变化，是新消息、已读行为，还是一次状态同步」
    NewMessage,
    MarkRead,
    Sync,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMsg {
    Ack {
        client_msg_id: String,
        message_id: Uuid,
    },
    NewMessage {
        chat_id: Uuid,
        message_id: Uuid,
        sender_id: Uuid,
        body: String,
        message_type: String,
        media_url: Option<String>,
        media_width: Option<i32>,
        media_height: Option<i32>,
        media_size_bytes: Option<i64>,
        media_content_type: Option<String>,
        created_at: DateTime<Utc>,
    },
    MessageRead {
        message_id: Uuid,
        user_id: Uuid,
        read_at: DateTime<Utc>,
    },
    UnreadDelta {
        chat_id: Uuid,
        delta: i64,
        unread_count: Option<u64>,
        reason: UnreadReason,
        seq: u64,
    },
    PresenceSync {
        chat_id: Uuid,
        online_user_ids: Vec<Uuid>,
    },
    PresenceChanged {
        chat_id: Uuid,
        user_id: Uuid,
        online: bool,
    },
    Error {
        code: String,
        message: String,
    },
    Pong,
}

pub fn try_text_message(message: &ServerMsg) -> Result<Message, serde_json::Error> {
    Ok(Message::Text(serde_json::to_string(message)?.into()))
}
