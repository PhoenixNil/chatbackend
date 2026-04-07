use axum::extract::ws::Message;
use tokio::sync::mpsc::{Sender, error::TrySendError};
use tracing::warn;
use uuid::Uuid;

use super::PresenceChange;
use super::presence;
use super::state::{ConnectionState, PendingDelivery, RoomsState};
use crate::websocket::protocol::{ServerMsg, UnreadReason, try_text_message};

pub(super) fn connect_user(
    state: &RoomsState,
    user_id: Uuid,
    connection_id: Uuid,
    sender: Sender<Message>,
) {
    state.user_sockets.entry(user_id).or_default().insert(
        connection_id,
        ConnectionState {
            sender,
            next_seq: 1,
        },
    );
}

pub(super) fn collect_deliveries(
    state: &RoomsState,
    user_ids: &[Uuid],
    message: &Message,
) -> Vec<PendingDelivery> {
    let mut deliveries = Vec::new();

    for user_id in user_ids {
        if let Some(connections) = state.user_sockets.get(user_id) {
            for (connection_id, connection) in connections.value().iter() {
                deliveries.push(PendingDelivery {
                    user_id: *user_id,
                    connection_id: *connection_id,
                    sender: connection.sender.clone(),
                    message: message.clone(),
                });
            }
        }
    }

    deliveries
}

pub(super) fn send_unread_delta_to_connection(
    state: &RoomsState,
    user_id: Uuid,
    connection_id: Uuid,
    chat_id: Uuid,
    delta: i64,
    unread_count: Option<u64>,
    reason: UnreadReason,
) -> (bool, Vec<PresenceChange>) {
    let send_info = if let Some(mut connections) = state.user_sockets.get_mut(&user_id) {
        if let Some(connection) = connections.get_mut(&connection_id) {
            let seq = connection.next_seq;
            connection.next_seq += 1;
            Some((connection.sender.clone(), seq))
        } else {
            None
        }
    } else {
        None
    };

    let Some((sender, seq)) = send_info else {
        return (false, Vec::new());
    };

    let message = match try_text_message(&ServerMsg::UnreadDelta {
        chat_id,
        delta,
        unread_count,
        reason,
        seq,
    }) {
        Ok(message) => message,
        Err(error) => {
            warn!(
                %error,
                %user_id,
                %connection_id,
                "failed to serialize unread delta message"
            );
            return (false, Vec::new());
        }
    };

    match sender.try_send(message) {
        Ok(()) => (true, Vec::new()),
        Err(TrySendError::Closed(_)) => {
            warn!(
                %user_id,
                %connection_id,
                "dropping websocket connection because outbound channel is closed"
            );
            let presence_broadcasts = presence::remove_connection(state, user_id, connection_id);
            (false, presence_broadcasts)
        }
        Err(TrySendError::Full(_)) => {
            warn!(
                %user_id,
                %connection_id,
                "outbound queue full, dropping unread delta (connection stays alive)"
            );
            (false, Vec::new())
        }
    }
}

pub(super) fn send_unread_delta_to_user(
    state: &RoomsState,
    user_id: Uuid,
    chat_id: Uuid,
    delta: i64,
    unread_count: Option<u64>,
    reason: UnreadReason,
) -> Vec<PresenceChange> {
    let sends: Vec<(Uuid, Sender<Message>, u64)> =
        if let Some(mut connections) = state.user_sockets.get_mut(&user_id) {
            connections
                .iter_mut()
                .map(|(conn_id, conn)| {
                    let seq = conn.next_seq;
                    conn.next_seq += 1;
                    (*conn_id, conn.sender.clone(), seq)
                })
                .collect()
        } else {
            return Vec::new();
        };

    // Drop the DashMap guard before sending — this is the key difference
    // from the old Mutex approach: the lock is released per-shard, and we
    // only held a ref to this single user's entry.

    let mut failed = Vec::new();
    for (connection_id, sender, seq) in sends {
        let message = match try_text_message(&ServerMsg::UnreadDelta {
            chat_id,
            delta,
            unread_count,
            reason,
            seq,
        }) {
            Ok(message) => message,
            Err(error) => {
                warn!(
                    %error,
                    %user_id,
                    %connection_id,
                    "failed to serialize unread delta message"
                );
                continue;
            }
        };

        match sender.try_send(message) {
            Ok(()) => {}
            Err(TrySendError::Closed(_)) => {
                warn!(
                    %user_id,
                    %connection_id,
                    "dropping websocket connection because outbound channel is closed"
                );
                failed.push(connection_id);
            }
            Err(TrySendError::Full(_)) => {
                warn!(
                    %user_id,
                    %connection_id,
                    "outbound queue full, dropping unread delta (connection stays alive)"
                );
            }
        }
    }

    let mut presence_broadcasts = Vec::new();
    for connection_id in failed {
        presence_broadcasts.extend(presence::remove_connection(state, user_id, connection_id));
    }

    presence_broadcasts
}

pub(super) fn deliver_one(
    state: &RoomsState,
    delivery: PendingDelivery,
) -> (bool, Vec<PresenceChange>) {
    match delivery.sender.try_send(delivery.message) {
        Ok(()) => (true, Vec::new()),
        Err(TrySendError::Closed(_)) => {
            warn!(
                user_id = %delivery.user_id,
                connection_id = %delivery.connection_id,
                "dropping websocket connection because outbound channel is closed"
            );
            let presence_broadcasts =
                presence::remove_connection(state, delivery.user_id, delivery.connection_id);
            (false, presence_broadcasts)
        }
        Err(TrySendError::Full(_)) => {
            warn!(
                user_id = %delivery.user_id,
                connection_id = %delivery.connection_id,
                "outbound queue full, dropping broadcast message (connection stays alive)"
            );
            (false, Vec::new())
        }
    }
}
