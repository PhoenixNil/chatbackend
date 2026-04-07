ALTER TABLE messages
    DROP COLUMN IF EXISTS media_content_type;

ALTER TABLE messages
    DROP COLUMN IF EXISTS media_size_bytes;

ALTER TABLE messages
    DROP COLUMN IF EXISTS media_height;

ALTER TABLE messages
    DROP COLUMN IF EXISTS media_width;

ALTER TABLE messages
    DROP COLUMN IF EXISTS media_url;

ALTER TABLE messages
    DROP COLUMN IF EXISTS message_type;
