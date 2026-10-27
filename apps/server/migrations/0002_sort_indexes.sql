-- Indexes for the sort orders added in 0.2 (`order:size`, `pixels`, `tags`,
-- `updated`). Like the others they cover live posts only; ORDER BY in
-- routes/posts.rs uses the same expressions.
CREATE INDEX posts_live_size ON posts (file_size DESC, id DESC) WHERE deleted_at IS NULL;
CREATE INDEX posts_live_pixels ON posts ((width::bigint * height) DESC NULLS LAST, id DESC)
    WHERE deleted_at IS NULL;
CREATE INDEX posts_live_tag_count ON posts (cardinality(tag_ids) DESC, id DESC)
    WHERE deleted_at IS NULL;
CREATE INDEX posts_live_updated ON posts (updated_at DESC, id DESC) WHERE deleted_at IS NULL;
