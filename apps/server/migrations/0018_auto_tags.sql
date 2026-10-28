-- When the server added its automatic metadata tags (`highres`, `video`, ...)
-- to the post. NULL: never, so the startup backfill tags it once. A tag a
-- user removes later doesn't come back.
ALTER TABLE posts ADD COLUMN meta_tagged_at TIMESTAMPTZ;
CREATE INDEX posts_meta_untagged ON posts (id) WHERE meta_tagged_at IS NULL;

-- Admin override for STARBOORU_AUTO_TAGS. NULL follows the variable.
ALTER TABLE server_settings ADD COLUMN auto_meta_tags BOOLEAN;
