-- A smaller JPEG copy of a big image, shown in the viewer instead of the
-- original. NULL: not looked at yet (the startup backfill does it);
-- false: the post has none, because it's small or not a still image.
ALTER TABLE posts ADD COLUMN has_sample BOOLEAN;

UPDATE posts SET has_sample = false
WHERE kind <> 0 OR ext NOT IN ('jpg', 'png', 'webp', 'bmp', 'tiff') OR file_size < 1048576;

CREATE INDEX posts_sample_pending ON posts (id) WHERE has_sample IS NULL;
