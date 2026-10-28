-- Background URL imports (`POST /imports`): the server downloads each
-- address on its own, so an import goes on with every app closed. Rows left
-- `running` by a restart go back to `queued` at startup; finished rows are
-- removed after a week.
CREATE TABLE import_jobs (
    id BIGSERIAL PRIMARY KEY,
    user_id BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    url TEXT NOT NULL,
    source TEXT,
    tags TEXT[] NOT NULL DEFAULT '{}',
    rating SMALLINT NOT NULL DEFAULT 0,
    state TEXT NOT NULL DEFAULT 'queued'
        CHECK (state IN ('queued', 'running', 'done', 'duplicate', 'failed')),
    post_id BIGINT REFERENCES posts(id) ON DELETE SET NULL,
    error TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    finished_at TIMESTAMPTZ
);
CREATE INDEX import_jobs_waiting ON import_jobs (id) WHERE state IN ('queued', 'running');
CREATE INDEX import_jobs_user ON import_jobs (user_id, id DESC);
