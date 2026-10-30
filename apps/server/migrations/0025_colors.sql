-- The colours of a picture, worked out from its thumbnail. `colors` holds the
-- OKLab grid cells it is searched by (starbooru_core::color), `palette` the
-- colours shown for it as 0xRRGGBB.
ALTER TABLE posts ADD COLUMN colors SMALLINT[] NOT NULL DEFAULT '{}';
ALTER TABLE posts ADD COLUMN palette INTEGER[] NOT NULL DEFAULT '{}';

-- Whether the thumbnail was looked at; the startup backfill reads the others.
ALTER TABLE posts ADD COLUMN colors_scanned BOOLEAN NOT NULL DEFAULT false;
CREATE INDEX posts_colors_unscanned ON posts (id) WHERE NOT colors_scanned;

-- `color:`, on live posts.
CREATE INDEX posts_live_colors ON posts USING GIN (colors) WHERE deleted_at IS NULL;
