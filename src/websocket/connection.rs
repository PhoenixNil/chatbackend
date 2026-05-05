use axum::extract::ws::{Message, WebSocket};
use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc::{Sender, channel, error::TrySendError};
use tracing::{debug, warn};
use uuid::Uuid;

use crate::state::{AppState, SharedAppState};
use crate::websocket::dispatch;
use crate::websocket::protocol::{ClientMsg, ServerMsg, UnreadReason, try_text_message};

const WS_OUTBOX_CAPACITY: usize = 256;

//帮当前这条 WebSocket 连接发消息
#[derive(Clone)]
struct LocalOutbox {
    user_id: Uuid,
    connection_id: Uuid,
    sender: Sender<Message>,
}

impl LocalOutbox {
    fn new(user_id: Uuid, connection_id: Uuid, sender: Sender<Message>) -> Self {
        Self {
            user_id,
            connection_id,
            sender,
        }
    }

    fn send(&self, message: ServerMsg) -> bool {
        let outbound = match try_text_message(&message) {
            Ok(outbound) => outbound,
            Err(error) => {
                warn!(
                    %error,
                    %self.user_id,
                    %self.connection_id,
                    "failed to serialize websocket response"
                );
                return false;
            }
        };

        match self.sender.try_send(outbound) {
            Ok(()) => true,
            Err(TrySendError::Closed(_)) => {
                warn!(
                    %self.user_id,
                    %self.connection_id,
                    "closing websocket connection because outbound channel is closed"
                );
                false
            }
            Err(TrySendError::Full(_)) => {
                warn!(
                    %self.user_id,
                    %self.connection_id,
                    capacity = WS_OUTBOX_CAPACITY,
                    "closing websocket connection because outbound queue is full"
                );
                false
            }
        }
    }

    fn disconnect(&self, state: &AppState) {
        let presence_broadcasts = state
            .rooms
            .disconnect_user(self.user_id, self.connection_id);
        state.rooms.broadcast_presence_changes(presence_broadcasts);
    }
}

pub async fn handle_socket(state: SharedAppState, user_id: Uuid, socket: WebSocket) {
    let connection_id = Uuid::new_v4();
    let (sender, mut out_rx) = channel::<Message>(WS_OUTBOX_CAPACITY);
    let outbox = LocalOutbox::new(user_id, connection_id, sender);
    let (mut ws_sender, mut ws_receiver) = socket.split();

    let writer_task = tokio::spawn(async move {
        while let Some(message) = out_rx.recv().await {
            if ws_sender.send(message).await.is_err() {
                break;
            }
        }
    });

    state
        .rooms
        .connect_user(user_id, connection_id, outbox.sender.clone());

    if !sync_initial_state(&state, &outbox).await {
        close_socket(&state, &outbox, &writer_task);
        debug!(
            %user_id,
            %connection_id,
            "websocket connection closed during initial unread sync"
        );
        return;
    }

    while let Some(next) = ws_receiver.next().await {
        let message = match next {
            Ok(message) => message,
            Err(error) => {
                warn!(%error, %user_id, "websocket receive error");
                break;
            }
        };

        let should_continue = match message {
            Message::Text(raw) => handle_text_message(&state, user_id, &outbox, &raw).await,
            Message::Ping(_) => outbox.send(ServerMsg::Pong),
            Message::Close(_) => false,
            _ => true,
        };

        if !should_continue {
            break;
        }
    }

    close_socket(&state, &outbox, &writer_task);
    debug!(%user_id, %connection_id, "websocket connection closed");
}

async fn sync_initial_state(state: &AppState, outbox: &LocalOutbox) -> bool {
    match state.chat_service.list_chats(outbox.user_id).await {
        Ok(chats) => {
            if !sync_presence_for_chat_ids(
                state,
                outbox,
                chats.iter().map(|chat| chat.id).collect(),
            ) {
                return false;
            }

            for chat in chats {
                let queued = state.rooms.send_unread_delta_to_connection(
                    outbox.user_id,
                    outbox.connection_id,
                    chat.id,
                    0,
                    Some(chat.unread_count),
                    UnreadReason::Sync,
                );

                if !queued {
                    return false;
                }
            }

            true
        }
        Err(error) => {
            warn!(
                %error,
                user_id = %outbox.user_id,
                "failed to preload user chats for websocket"
            );
            true
        }
    }
}

fn sync_presence_for_chat_ids(state: &AppState, outbox: &LocalOutbox, chat_ids: Vec<Uuid>) -> bool {
    let presence_sync = state.rooms.sync_user_chats(outbox.user_id, chat_ids);

    for snapshot in presence_sync.snapshots {
        if !outbox.send(ServerMsg::PresenceSync {
            chat_id: snapshot.chat_id,
            online_user_ids: snapshot.online_user_ids,
        }) {
            return false;
        }
    }

    state
        .rooms
        .broadcast_presence_changes(presence_sync.broadcasts);
    true
}

async fn sync_presence_for_user(state: &AppState, outbox: &LocalOutbox) -> bool {
    match state.chat_service.list_chats(outbox.user_id).await {
        Ok(chats) => sync_presence_for_chat_ids(
            state,
            outbox,
            chats.into_iter().map(|chat| chat.id).collect(),
        ),
        Err(error) => outbox.send(ServerMsg::Error {
            code: "PRESENCE_SYNC_FAILED".to_string(),
            message: format!("failed to sync presence: {error}"),
        }),
    }
}

async fn handle_text_message(
    state: &AppState,
    user_id: Uuid,
    outbox: &LocalOutbox,
    raw: &str,
) -> bool {
    let client_msg = match serde_json::from_str::<ClientMsg>(raw) {
        Ok(msg) => msg,
        Err(error) => {
            return outbox.send(ServerMsg::Error {
                code: "BAD_REQUEST".to_string(),
                message: format!("invalid websocket payload: {error}"),
            });
        }
    };

    if matches!(client_msg, ClientMsg::SyncPresence) {
        return sync_presence_for_user(state, outbox).await;
    }

    match dispatch::dispatch(state, user_id, client_msg).await {
        Ok(outcome) => {
            for direct_msg in outcome.direct_messages {
                if !outbox.send(direct_msg) {
                    return false;
                }
            }

            if let Some(packet) = outcome.broadcast {
                state
                    .rooms
                    .broadcast_to_users(&packet.user_ids, packet.message);
            }

            let empty_unread_counts = std::collections::HashMap::new();
            for unread_event in outcome.unread_events {
                state.rooms.send_unread_delta_to_users(
                    &unread_event.user_ids,
                    unread_event.chat_id,
                    unread_event.delta,
                    unread_event.reason,
                    &empty_unread_counts,
                );
            }

            true
        }
        Err(error) => outbox.send(ServerMsg::Error {
            code: error.code().to_string(),
            message: error.client_message(),
        }),
    }
}

fn close_socket(state: &AppState, outbox: &LocalOutbox, writer_task: &tokio::task::JoinHandle<()>) {
    outbox.disconnect(state);
    writer_task.abort();
}
