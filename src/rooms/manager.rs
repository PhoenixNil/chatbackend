#[path = "manager/delivery.rs"]
mod delivery;
#[path = "manager/presence.rs"]
mod presence;
#[path = "manager/state.rs"]
mod state;
#[cfg(test)]
#[path = "manager/tests.rs"]
mod tests;

use std::collections::HashMap;

use axum::extract::ws::Message;
use tokio::sync::mpsc::Sender;
use tracing::warn;
use uuid::Uuid;

use crate::websocket::protocol::{ServerMsg, UnreadReason, try_text_message};

#[derive(Debug, Clone)]
pub struct PresenceChange {
    pub user_ids: Vec<Uuid>,
    pub chat_id: Uuid,
    pub user_id: Uuid,
    pub online: bool,
}

#[derive(Debug, Clone)]
pub struct PresenceSnapshot {
    pub chat_id: Uuid,
    pub online_user_ids: Vec<Uuid>,
}

#[derive(Debug, Clone, Default)]
pub struct PresenceSyncResult {
    pub snapshots: Vec<PresenceSnapshot>,
    pub broadcasts: Vec<PresenceChange>,
}

#[derive(Clone, Default)]
pub struct RoomManager {
    state: state::SharedRoomsState,
}

impl RoomManager {
    pub fn connect_user(&self, user_id: Uuid, connection_id: Uuid, sender: Sender<Message>) {
        delivery::connect_user(&self.state, user_id, connection_id, sender);
    }

    pub fn disconnect_user(&self, user_id: Uuid, connection_id: Uuid) -> Vec<PresenceChange> {
        presence::remove_connection(&self.state, user_id, connection_id)
    }

    pub fn sync_user_chats(&self, user_id: Uuid, chat_ids: Vec<Uuid>) -> PresenceSyncResult {
        presence::sync_user_chats(&self.state, user_id, chat_ids)
    }

    pub fn broadcast_to_users(&self, user_ids: &[Uuid], message: ServerMsg) {
        let message = match try_text_message(&message) {
            Ok(message) => message,
            Err(error) => {
                warn!(%error, "failed to serialize websocket broadcast message");
                return;
            }
        };

        let deliveries = delivery::collect_deliveries(&self.state, user_ids, &message);
        self.deliver(deliveries);
    }

    pub fn broadcast_presence_changes(&self, changes: Vec<PresenceChange>) {
        for change in changes {
            if change.user_ids.is_empty() {
                continue;
            }

            self.broadcast_to_users(
                &change.user_ids,
                ServerMsg::PresenceChanged {
                    chat_id: change.chat_id,
                    user_id: change.user_id,
                    online: change.online,
                },
            );
        }
    }

    pub fn send_unread_delta_to_connection(
        &self,
        user_id: Uuid,
        connection_id: Uuid,
        chat_id: Uuid,
        delta: i64,
        unread_count: Option<u64>,
        reason: UnreadReason,
    ) -> bool {
        let (result, presence_broadcasts) = delivery::send_unread_delta_to_connection(
            &self.state,
            user_id,
            connection_id,
            chat_id,
            delta,
            unread_count,
            reason,
        );

        self.broadcast_presence_changes(presence_broadcasts);
        result
    }

    pub fn send_unread_delta_to_user(
        &self,
        user_id: Uuid,
        chat_id: Uuid,
        delta: i64,
        unread_count: Option<u64>,
        reason: UnreadReason,
    ) {
        let presence_broadcasts = delivery::send_unread_delta_to_user(
            &self.state,
            user_id,
            chat_id,
            delta,
            unread_count,
            reason,
        );

        self.broadcast_presence_changes(presence_broadcasts);
    }

    pub fn send_unread_delta_to_users(
        &self,
        user_ids: &[Uuid],
        chat_id: Uuid,
        delta: i64,
        reason: UnreadReason,
        unread_count_by_user: &HashMap<Uuid, u64>,
    ) {
        for user_id in user_ids {
            let unread_count = unread_count_by_user.get(user_id).copied();
            self.send_unread_delta_to_user(*user_id, chat_id, delta, unread_count, reason);
        }
    }

    fn deliver(&self, deliveries: Vec<state::PendingDelivery>) {
        for delivery in deliveries {
            let (_, presence_broadcasts) = delivery::deliver_one(&self.state, delivery);
            self.broadcast_presence_changes(presence_broadcasts);
        }
    }
}
