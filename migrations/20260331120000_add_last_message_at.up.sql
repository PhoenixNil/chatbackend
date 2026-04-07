ALTER TABLE chats
  ADD COLUMN last_message_at TIMESTAMPTZ NOT NULL DEFAULT NOW();

-- Back-fill: set last_message_at to the latest message time, or keep created_at
UPDATE chats
SET last_message_at = COALESCE(
  (SELECT MAX(m.created_at) FROM messages m WHERE m.chat_id = chats.id),
  chats.created_at
);
