ALTER TABLE messages
    ADD COLUMN IF NOT EXISTS message_type TEXT NOT NULL DEFAULT 'text';

ALTER TABLE messages
    ADD COLUMN IF NOT EXISTS media_url TEXT;

ALTER TABLE messages
    ADD COLUMN IF NOT EXISTS media_width INTEGER;

ALTER TABLE messages
    ADD COLUMN IF NOT EXISTS media_height INTEGER;

ALTER TABLE messages
    ADD COLUMN IF NOT EXISTS media_size_bytes BIGINT;

ALTER TABLE messages
    ADD COLUMN IF NOT EXISTS media_content_type TEXT;

UPDATE messages
SET message_type = 'text'
WHERE message_type IS NULL
   OR message_type = '';
