-- Thumbnails used to fit in 512 pixels whatever the shape, which left a long
-- picture far too narrow to look sharp on a tile. This marks the ones to make
-- again; a task at startup does that and clears the mark.
ALTER TABLE posts ADD COLUMN thumb_stale BOOLEAN NOT NULL DEFAULT false;

UPDATE posts SET thumb_stale = true
WHERE has_thumb
  AND GREATEST(width, height) > 2 * LEAST(width, height)
  AND GREATEST(width, height) > 512;

CREATE INDEX posts_thumb_stale_idx ON posts (id) WHERE thumb_stale;
