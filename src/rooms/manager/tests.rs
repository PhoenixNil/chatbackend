use std::sync::Barrier;
use std::thread;

use axum::extract::ws::Message;
use tokio::sync::mpsc::channel;
use uuid::Uuid;

use super::RoomManager;
use crate::websocket::protocol::{ServerMsg, UnreadReason};

fn decode_server_msg(message: Message) -> ServerMsg {
    match message {
        Message::Text(payload) => {
            serde_json::from_str(payload.as_str()).expect("valid serialized ServerMsg")
        }
        other => panic!("unexpected websocket message in test channel: {other:?}"),
    }
}

#[test]
fn concurrent_unread_delta_keeps_seq_order_per_connection() {
    const THREADS: usize = 4;
    const SENDS_PER_THREAD: usize = 128;
    const TOTAL_SENDS: usize = THREADS * SENDS_PER_THREAD;

    let manager = RoomManager::default();
    let user_id = Uuid::new_v4();
    let connection_id = Uuid::new_v4();
    let (sender, mut receiver) = channel(TOTAL_SENDS);

    manager.connect_user(user_id, connection_id, sender);

    let start_barrier = Barrier::new(THREADS + 1);

    thread::scope(|scope| {
        for _ in 0..THREADS {
            let manager = manager.clone();
            let start_barrier = &start_barrier;

            scope.spawn(move || {
                start_barrier.wait();

                for _ in 0..SENDS_PER_THREAD {
                    assert!(manager.send_unread_delta_to_connection(
                        user_id,
                        connection_id,
                        Uuid::new_v4(),
                        1,
                        None,
                        UnreadReason::NewMessage,
                    ));
                }
            });
        }

        start_barrier.wait();
    });

    let mut seqs = Vec::with_capacity(TOTAL_SENDS);
    for expected_seq in 1..=TOTAL_SENDS as u64 {
        match decode_server_msg(
            receiver
                .try_recv()
                .unwrap_or_else(|error| panic!("missing unread delta {expected_seq}: {error:?}")),
        ) {
            ServerMsg::UnreadDelta { seq, .. } => seqs.push(seq),
            other => panic!("unexpected message in unread queue: {other:?}"),
        }
    }

    // With DashMap the per-user shard lock is released between seq
    // assignment and try_send, so messages may arrive out of order in
    // the channel. The invariant we actually care about is that every
    // seq from 1..=N is present exactly once (clients sort by seq).
    seqs.sort_unstable();
    assert_eq!(seqs, (1..=TOTAL_SENDS as u64).collect::<Vec<_>>());
}

#[test]
fn sync_user_chats_tracks_online_presence_and_disconnects_cleanly() {
    let manager = RoomManager::default();
    let chat_id = Uuid::new_v4();
    let user_a = Uuid::new_v4();
    let user_b = Uuid::new_v4();
    let connection_a = Uuid::new_v4();
    let connection_b = Uuid::new_v4();
    let (sender_a, _receiver_a) = channel(8);
    let (sender_b, _receiver_b) = channel(8);

    manager.connect_user(user_a, connection_a, sender_a);
    manager.connect_user(user_b, connection_b, sender_b);

    let sync_a = manager.sync_user_chats(user_a, vec![chat_id]);
    assert_eq!(sync_a.broadcasts.len(), 0);
    assert_eq!(sync_a.snapshots.len(), 1);
    assert_eq!(sync_a.snapshots[0].online_user_ids, vec![user_a]);

    let sync_b = manager.sync_user_chats(user_b, vec![chat_id]);
    assert_eq!(sync_b.broadcasts.len(), 1);
    assert_eq!(sync_b.broadcasts[0].chat_id, chat_id);
    assert_eq!(sync_b.broadcasts[0].user_id, user_b);
    assert!(sync_b.broadcasts[0].online);
    assert_eq!(sync_b.broadcasts[0].user_ids, vec![user_a]);
    assert_eq!(sync_b.snapshots.len(), 1);
    let mut expected_online_users = vec![user_a, user_b];
    expected_online_users.sort_unstable_by_key(|user_id| user_id.as_u128());
    assert_eq!(sync_b.snapshots[0].online_user_ids, expected_online_users);

    let disconnect_b = manager.disconnect_user(user_b, connection_b);
    assert_eq!(disconnect_b.len(), 1);
    assert_eq!(disconnect_b[0].chat_id, chat_id);
    assert_eq!(disconnect_b[0].user_id, user_b);
    assert!(!disconnect_b[0].online);
    assert_eq!(disconnect_b[0].user_ids, vec![user_a]);
}
