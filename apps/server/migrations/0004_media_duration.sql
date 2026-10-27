-- Length of videos, from ffprobe. NULL for images and when unknown.
ALTER TABLE posts ADD COLUMN duration_ms BIGINT;
