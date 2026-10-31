-- Imports of a whole library from another booru (`POST /library-imports`).
-- The server walks the other site on its own, so an import goes on with every
-- app closed and after a restart. `login` and `secret` are that site's account:
-- kept only while the import is waiting or running, so a restart can go on, and
-- cleared when it ends.
CREATE TABLE library_imports (
    id BIGSERIAL PRIMARY KEY,
    user_id BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    source TEXT NOT NULL,
    base TEXT NOT NULL,
    login TEXT,
    secret TEXT,
    unrated SMALLINT NOT NULL,
    state TEXT NOT NULL DEFAULT 'queued'
        CHECK (state IN ('queued', 'running', 'done', 'stopped', 'failed')),
    phase TEXT NOT NULL DEFAULT 'rules' CHECK (phase IN ('rules', 'posts', 'groups')),
    -- Where in the phase's walk to go on, in terms only the source understands.
    position TEXT NOT NULL DEFAULT '',
    total BIGINT,
    imported BIGINT NOT NULL DEFAULT 0,
    merged BIGINT NOT NULL DEFAULT 0,
    failed BIGINT NOT NULL DEFAULT 0,
    rules BIGINT NOT NULL DEFAULT 0,
    note TEXT,
    error TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    finished_at TIMESTAMPTZ,
    CHECK (state IN ('queued', 'running') OR (login IS NULL AND secret IS NULL))
);
CREATE INDEX library_imports_waiting ON library_imports (id) WHERE state IN ('queued', 'running');
CREATE INDEX library_imports_user ON library_imports (user_id, id DESC);

-- Every post an import has dealt with: `error` when it failed, so a retry tries
-- it again and skips the rest; `post_id` finds it again to add a pool's tag.
CREATE TABLE library_import_items (
    import_id BIGINT NOT NULL REFERENCES library_imports(id) ON DELETE CASCADE,
    remote_id TEXT NOT NULL,
    post_id BIGINT REFERENCES posts(id) ON DELETE SET NULL,
    error TEXT,
    PRIMARY KEY (import_id, remote_id)
);
CREATE INDEX library_import_items_post ON library_import_items (post_id) WHERE post_id IS NOT NULL;
