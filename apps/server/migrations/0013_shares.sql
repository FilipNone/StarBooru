-- Share links: anyone with the token sees the listed posts, in that order,
-- until `expires_at` (never when NULL) or until the owner removes the link.
-- The token is kept as is so its owner can copy the link again; it grants
-- nothing but viewing these posts.
CREATE TABLE shares (
    id             BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    user_id        BIGINT      NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    token          TEXT        NOT NULL UNIQUE,
    title          TEXT,
    post_ids       BIGINT[]    NOT NULL,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at     TIMESTAMPTZ,
    views          BIGINT      NOT NULL DEFAULT 0,
    last_viewed_at TIMESTAMPTZ
);
CREATE INDEX shares_user ON shares (user_id, created_at DESC);
