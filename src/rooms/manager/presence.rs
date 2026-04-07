use std::collections::HashSet;

use uuid::Uuid;

use super::state::RoomsState;
use super::{PresenceChange, PresenceSnapshot, PresenceSyncResult};

pub(super) fn sync_user_chats(
    state: &RoomsState,
    user_id: Uuid,
    chat_ids: Vec<Uuid>,
) -> PresenceSyncResult {
    let has_active_connection = state
        .user_sockets
        .get(&user_id)
        .is_some_and(|connections| !connections.is_empty());

    let new_chats: HashSet<Uuid> = chat_ids.into_iter().collect();

    let previous_chats = state
        .user_chats
        .get(&user_id)
        .map(|entry| entry.value().clone())
        .unwrap_or_default();

    let mut broadcasts = Vec::new();

    for chat_id in previous_chats
        .difference(&new_chats)
        .copied()
        .collect::<Vec<_>>()
    {
        let recipients = mark_user_offline_in_chat(state, chat_id, user_id);
        if !recipients.is_empty() {
            broadcasts.push(PresenceChange {
                user_ids: recipients,
                chat_id,
                user_id,
                online: false,
            });
        }
    }

    if has_active_connection {
        for chat_id in new_chats
            .difference(&previous_chats)
            .copied()
            .collect::<Vec<_>>()
        {
            let recipients = mark_user_online_in_chat(state, chat_id, user_id);
            if !recipients.is_empty() {
                broadcasts.push(PresenceChange {
                    user_ids: recipients,
                    chat_id,
                    user_id,
                    online: true,
                });
            }
        }
    }

    state.user_chats.insert(user_id, new_chats.clone());

    let mut snapshots = new_chats
        .into_iter()
        .map(|chat_id| PresenceSnapshot {
            chat_id,
            online_user_ids: online_users_for_chat(state, chat_id),
        })
        .collect::<Vec<_>>();
    snapshots.sort_unstable_by_key(|snapshot| snapshot.chat_id.as_u128());

    PresenceSyncResult {
        snapshots,
        broadcasts,
    }
}

pub(super) fn remove_connection(
    state: &RoomsState,
    user_id: Uuid,
    connection_id: Uuid,
) -> Vec<PresenceChange> {
    // Use the entry API so the check-and-remove-if-empty is atomic
    // within the same shard lock.
    let should_clear = match state.user_sockets.entry(user_id) {
        dashmap::mapref::entry::Entry::Occupied(mut entry) => {
            entry.get_mut().remove(&connection_id);
            if entry.get().is_empty() {
                entry.remove();
                true
            } else {
                false
            }
        }
        dashmap::mapref::entry::Entry::Vacant(_) => false,
    };

    if should_clear {
        return clear_user_presence(state, user_id);
    }

    Vec::new()
}

/// Remove the user from all chat presence sets. The user_sockets entry
/// has already been cleaned up by the caller.
fn clear_user_presence(state: &RoomsState, user_id: Uuid) -> Vec<PresenceChange> {
    let mut broadcasts = Vec::new();
    if let Some((_, chats)) = state.user_chats.remove(&user_id) {
        for chat_id in chats {
            let recipients = mark_user_offline_in_chat(state, chat_id, user_id);
            if !recipients.is_empty() {
                broadcasts.push(PresenceChange {
                    user_ids: recipients,
                    chat_id,
                    user_id,
                    online: false,
                });
            }
        }
    }

    broadcasts
}

fn mark_user_online_in_chat(state: &RoomsState, chat_id: Uuid, user_id: Uuid) -> Vec<Uuid> {
    let mut online_users = state.chat_online_users.entry(chat_id).or_default();
    let recipients = online_users
        .iter()
        .copied()
        .filter(|online_user_id| *online_user_id != user_id)
        .collect::<Vec<_>>();
    let inserted = online_users.insert(user_id);

    if inserted { recipients } else { Vec::new() }
}

fn mark_user_offline_in_chat(state: &RoomsState, chat_id: Uuid, user_id: Uuid) -> Vec<Uuid> {
    match state.chat_online_users.entry(chat_id) {
        dashmap::mapref::entry::Entry::Occupied(mut entry) => {
            let removed = entry.get_mut().remove(&user_id);
            let recipients = entry.get().iter().copied().collect::<Vec<_>>();
            if entry.get().is_empty() {
                entry.remove();
            }
            if removed { recipients } else { Vec::new() }
        }
        dashmap::mapref::entry::Entry::Vacant(_) => Vec::new(),
    }
}

fn online_users_for_chat(state: &RoomsState, chat_id: Uuid) -> Vec<Uuid> {
    let mut online_user_ids = state
        .chat_online_users
        .get(&chat_id)
        .map(|users| users.iter().copied().collect::<Vec<_>>())
        .unwrap_or_default();
    online_user_ids.sort_unstable_by_key(|user_id| user_id.as_u128());
    online_user_ids
}
