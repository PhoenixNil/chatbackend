use uuid::Uuid;
use validator::Validate;

use crate::errors::AppError;
use crate::state::AppState;
use crate::websocket::protocol::{ClientMsg, ServerMsg, UnreadReason};

#[derive(Debug, Validate)]
struct SendMessageRequest {
    #[validate(length(min = 1, max = 64))]
    #[validate(custom(function = "crate::validation::validate_trimmed_not_empty"))]
    client_msg_id: String,
    #[validate(length(min = 1, max = 2000))]
    #[validate(custom(function = "crate::validation::validate_trimmed_not_empty"))]
    body: String,
}

#[derive(Debug, Clone)]
pub struct BroadcastPacket {
    pub user_ids: Vec<Uuid>,
    pub message: ServerMsg,
}

#[derive(Debug, Clone)]
pub struct UnreadEventIntent {
    pub user_ids: Vec<Uuid>,
    pub chat_id: Uuid,
    pub delta: i64, //未读消息的“增量变化
    pub reason: UnreadReason,
}

#[derive(Debug, Clone, Default)]
pub struct DispatchOutcome {
    pub direct_messages: Vec<ServerMsg>,
    pub broadcast: Option<BroadcastPacket>,
    pub unread_events: Vec<UnreadEventIntent>,
}

pub async fn dispatch(
    state: &AppState,
    user_id: Uuid,
    msg: ClientMsg,
) -> Result<DispatchOutcome, AppError> {
    match msg {
        ClientMsg::SendMessage {
            chat_id,
            client_msg_id,
            body,
        } => {
            SendMessageRequest {
                client_msg_id: client_msg_id.clone(),
                body: body.clone(),
            }
            .validate()?;

            let sent = state
                .chat_service
                .send_message(user_id, chat_id, body)
                .await?;
            let direct_messages = vec![ServerMsg::Ack {
                client_msg_id,
                message_id: sent.message.id,
            }];

            let broadcast = BroadcastPacket {
                user_ids: sent.member_ids,
                message: ServerMsg::NewMessage {
                    chat_id: sent.message.chat_id,
                    message_id: sent.message.id,
                    sender_id: sent.message.sender_id,
                    body: sent.message.body,
                    message_type: sent.message.message_type,
                    media_url: sent.message.media_url,
                    media_width: sent.message.media_width,
                    media_height: sent.message.media_height,
                    media_size_bytes: sent.message.media_size_bytes,
                    media_content_type: sent.message.media_content_type,
                    created_at: sent.message.created_at,
                },
            };

            let unread_events = vec![UnreadEventIntent {
                user_ids: broadcast
                    .user_ids
                    .iter()
                    .copied()
                    .filter(|member_id| *member_id != user_id)
                    .collect(),
                chat_id: sent.message.chat_id,
                delta: 1,
                reason: UnreadReason::NewMessage,
            }];

            Ok(DispatchOutcome {
                direct_messages,
                broadcast: Some(broadcast),
                unread_events,
            })
        }
        ClientMsg::MarkRead { message_id } => {
            let read = state.chat_service.mark_read(user_id, message_id).await?;
            let message_id = read.message_id;
            let chat_id = read.chat_id;
            let reader_id = read.user_id;
            let read_at = read.read_at;
            let broadcast = BroadcastPacket {
                // Keep a self-echo read receipt so the reader can confirm local
                // read state without fanning this event out to the whole chat.
                user_ids: vec![user_id],
                message: ServerMsg::MessageRead {
                    message_id,
                    user_id: reader_id,
                    read_at,
                },
            };

            let unread_events = if read.delta != 0 {
                vec![UnreadEventIntent {
                    user_ids: vec![user_id],
                    chat_id,
                    delta: read.delta,
                    reason: UnreadReason::MarkRead,
                }]
            } else {
                Vec::new()
            };

            Ok(DispatchOutcome {
                direct_messages: Vec::new(),
                broadcast: Some(broadcast),
                unread_events,
            })
        }
        ClientMsg::Ping => Ok(DispatchOutcome {
            direct_messages: vec![ServerMsg::Pong],
            broadcast: None,
            unread_events: Vec::new(),
        }),
        ClientMsg::SyncPresence => Ok(DispatchOutcome::default()),
    }
}
