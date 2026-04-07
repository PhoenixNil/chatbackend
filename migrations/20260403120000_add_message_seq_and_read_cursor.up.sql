ALTER TABLE messages
    ADD COLUMN IF NOT EXISTS seq BIGINT;

CREATE SEQUENCE IF NOT EXISTS messages_seq_seq;

ALTER SEQUENCE messages_seq_seq OWNED BY messages.seq;

ALTER TABLE messages
    ALTER COLUMN seq SET DEFAULT nextval('messages_seq_seq');

WITH ordered AS (
    SELECT id, nextval('messages_seq_seq') AS next_seq
    FROM messages
    WHERE seq IS NULL
    ORDER BY created_at, id
)
UPDATE messages m
SET seq = ordered.next_seq
FROM ordered
WHERE m.id = ordered.id;

ALTER TABLE messages
    ALTER COLUMN seq SET NOT NULL;

ALTER TABLE chat_members
    ADD COLUMN IF NOT EXISTS last_read_seq BIGINT NOT NULL DEFAULT 0;

UPDATE chat_members cm
SET last_read_seq = COALESCE((
    SELECT MAX(m.seq)
    FROM messages m
    JOIN message_reads mr
      ON mr.message_id = m.id
     AND mr.user_id = cm.user_id
    WHERE m.chat_id = cm.chat_id
), 0);

CREATE INDEX IF NOT EXISTS idx_messages_chat_id_seq_desc
    ON messages (chat_id, seq DESC);
