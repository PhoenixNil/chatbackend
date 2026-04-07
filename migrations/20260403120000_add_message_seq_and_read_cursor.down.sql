DROP INDEX IF EXISTS idx_messages_chat_id_seq_desc;

ALTER TABLE chat_members
    DROP COLUMN IF EXISTS last_read_seq;

ALTER TABLE messages
    ALTER COLUMN seq DROP DEFAULT;

ALTER TABLE messages
    DROP COLUMN IF EXISTS seq;

DROP SEQUENCE IF EXISTS messages_seq_seq;
