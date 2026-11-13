-- Searches on other boorus that a user kept: the search as typed and the sites
-- it goes to (`sites` is a JSON list of `{api, base}`).
CREATE TABLE remote_searches (
    id BIGSERIAL PRIMARY KEY,
    user_id BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    query TEXT NOT NULL,
    sites TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX remote_searches_user ON remote_searches (user_id, id);

-- A result of such a search is marked when it was already imported, which is
-- known by its page being a post's source. A hash index, as a source can be
-- longer than a B-tree entry may be.
CREATE INDEX posts_live_source ON posts USING hash (source)
    WHERE source IS NOT NULL AND deleted_at IS NULL;
