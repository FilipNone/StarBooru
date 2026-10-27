-- Searches a user keeps, shown on their gallery page.
CREATE TABLE saved_searches (
    id         BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    user_id    BIGINT      NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    name       TEXT        NOT NULL,
    query      TEXT        NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX saved_searches_user_name ON saved_searches (user_id, lower(name));
