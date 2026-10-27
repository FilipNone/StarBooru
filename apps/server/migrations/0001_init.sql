-- StarBooru initial schema.
-- Tags live denormalised on posts.tag_ids (GIN-indexed) so tag searches are a
-- single index scan: `tag_ids @> include AND NOT tag_ids && exclude`.

CREATE TABLE users (
    id            BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    username      TEXT        NOT NULL,
    password_hash TEXT        NOT NULL,
    role          TEXT        NOT NULL DEFAULT 'user' CHECK (role IN ('admin', 'user')),
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX users_username_ci ON users (lower(username));

CREATE TABLE sessions (
    token_hash BYTEA       PRIMARY KEY,
    user_id    BIGINT      NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at TIMESTAMPTZ NOT NULL
);
CREATE INDEX sessions_user ON sessions (user_id);
CREATE INDEX sessions_expiry ON sessions (expires_at);

CREATE TABLE tags (
    id         BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    name       TEXT        NOT NULL UNIQUE,
    category   SMALLINT    NOT NULL DEFAULT 0,
    post_count INTEGER     NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX tags_name_prefix ON tags (name text_pattern_ops);
CREATE INDEX tags_popular ON tags (post_count DESC);

CREATE TABLE posts (
    id          BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    sha256      BYTEA       NOT NULL UNIQUE,
    ext         TEXT        NOT NULL,
    mime        TEXT        NOT NULL,
    kind        SMALLINT    NOT NULL,
    file_size   BIGINT      NOT NULL,
    width       INTEGER,
    height      INTEGER,
    has_thumb   BOOLEAN     NOT NULL DEFAULT false,
    rating      SMALLINT    NOT NULL DEFAULT 0,
    source      TEXT,
    score       INTEGER     NOT NULL DEFAULT 0,
    tag_ids     BIGINT[]    NOT NULL DEFAULT '{}',
    uploader_id BIGINT      REFERENCES users (id) ON DELETE SET NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- Soft delete: non-null means the post is in the recycle bin.
    deleted_at  TIMESTAMPTZ
);
CREATE INDEX posts_tag_ids ON posts USING GIN (tag_ids);
CREATE INDEX posts_live_id ON posts (id DESC) WHERE deleted_at IS NULL;
CREATE INDEX posts_live_score ON posts (score DESC, id DESC) WHERE deleted_at IS NULL;

CREATE TABLE post_votes (
    post_id BIGINT   NOT NULL REFERENCES posts (id) ON DELETE CASCADE,
    user_id BIGINT   NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    value   SMALLINT NOT NULL CHECK (value IN (-1, 1)),
    PRIMARY KEY (post_id, user_id)
);
