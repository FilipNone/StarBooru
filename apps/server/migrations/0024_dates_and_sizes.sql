-- The date a file carries: EXIF for images, the container's creation time for
-- videos. `date:` falls back to the upload date without one. An EXIF date has
-- no time zone, so its wall clock is stored as if it were UTC, and the day
-- it shows is the day that matches.
ALTER TABLE posts ADD COLUMN file_date TIMESTAMPTZ;

-- Whether the file was looked at; the startup backfill reads the others once.
ALTER TABLE posts ADD COLUMN file_date_scanned BOOLEAN NOT NULL DEFAULT false;
CREATE INDEX posts_file_date_unscanned ON posts (id) WHERE NOT file_date_scanned;

-- The version of the automatic meta tag rules a post has had. A later version
-- adds only its new tags, so a tag someone removed doesn't come back. Posts
-- tagged so far had the first version.
ALTER TABLE posts ADD COLUMN meta_version SMALLINT NOT NULL DEFAULT 0;
UPDATE posts SET meta_version = 1 WHERE meta_tagged_at IS NOT NULL;
DROP INDEX posts_meta_untagged;
CREATE INDEX posts_meta_version ON posts (meta_version, id);

-- `date:`, `width:`, `height:`, `ratio:` and `order:longside`, on live posts.
-- `mpixels:` uses the pixels index of migration 0002.
CREATE INDEX posts_live_date ON posts (COALESCE(file_date, created_at)) WHERE deleted_at IS NULL;
CREATE INDEX posts_live_width ON posts (width) WHERE deleted_at IS NULL;
CREATE INDEX posts_live_height ON posts (height) WHERE deleted_at IS NULL;
CREATE INDEX posts_live_ratio ON posts ((width::bigint * 1000 / NULLIF(height, 0)))
    WHERE deleted_at IS NULL;
CREATE INDEX posts_live_long_side ON posts (GREATEST(width, height) DESC NULLS LAST, id DESC)
    WHERE deleted_at IS NULL;
