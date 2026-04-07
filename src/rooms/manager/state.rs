use axum::extract::ws::Message;
use dashmap::DashMap;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use tokio::sync::mpsc::Sender;
use uuid::Uuid;

#[derive(Clone)]
pub(super) struct ConnectionState {
    pub sender: Sender<Message>,
    pub next_seq: u64,
}

#[derive(Clone)]
pub(super) struct PendingDelivery {
    pub user_id: Uuid,
    pub connection_id: Uuid,
    pub sender: Sender<Message>,
    pub message: Message,
}

/// Per-field concurrent state. Each `DashMap` locks only the shard that
/// contains the key being accessed, so unrelated users / chats never
/// contend with each other.
pub(super) struct RoomsState {
    /// user_id → (connection_id → ConnectionState)
    pub user_sockets: DashMap<Uuid, HashMap<Uuid, ConnectionState>>,
    /// chat_id → set of online user_ids
    pub chat_online_users: DashMap<Uuid, HashSet<Uuid>>,
    /// user_id → set of chat_ids the user has joined
    pub user_chats: DashMap<Uuid, HashSet<Uuid>>,
}

impl Default for RoomsState {
    fn default() -> Self {
        Self {
            user_sockets: DashMap::new(),
            chat_online_users: DashMap::new(),
            user_chats: DashMap::new(),
        }
    }
}

pub(super) type SharedRoomsState = Arc<RoomsState>;
