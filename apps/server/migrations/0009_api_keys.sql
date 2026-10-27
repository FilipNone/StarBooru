-- API keys: long-lived tokens for scripts and other tools. Like sessions,
-- only a SHA-256 of the token is kept; `prefix` is the start of the token,
-- shown so people can tell their keys apart.
CREATE TABLE api_keys (
    id           BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    user_id      BIGINT      NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    name         TEXT        NOT NULL,
    token_hash   BYTEA       NOT NULL UNIQUE,
    prefix       TEXT        NOT NULL,
    read_only    BOOLEAN     NOT NULL DEFAULT false,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_used_at TIMESTAMPTZ,
    expires_at   TIMESTAMPTZ
);
CREATE INDEX api_keys_user ON api_keys (user_id);
