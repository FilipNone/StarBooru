-- Tags form a tree: searching a tag finds posts with any tag below it, and
-- the top-level tags are the sections the viewer groups general tags under.
ALTER TABLE tags ADD COLUMN parent_id BIGINT REFERENCES tags (id) ON DELETE SET NULL;
CREATE INDEX tags_parent ON tags (parent_id) WHERE parent_id IS NOT NULL;

-- The starter vocabulary (`starbooru_core::vocab`) seeded so far; the
-- server adds what newer versions bring, once.
ALTER TABLE server_settings ADD COLUMN vocab_version INTEGER NOT NULL DEFAULT 0;
