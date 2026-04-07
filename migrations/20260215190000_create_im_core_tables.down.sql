DROP INDEX IF EXISTS idx_message_reads_message_id_user_id;
DROP INDEX IF EXISTS idx_chat_members_user_id_chat_id;
DROP INDEX IF EXISTS idx_messages_chat_id_created_at_desc;

DROP TABLE IF EXISTS message_reads;
DROP TABLE IF EXISTS messages;
DROP TABLE IF EXISTS chat_members;
DROP TABLE IF EXISTS chats;
DROP TABLE IF EXISTS users;
